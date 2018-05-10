extern crate termion;
extern crate ransid;
extern crate failure;
extern crate libc;
extern crate crossbeam_channel;

use failure::Error;
use termion::event::*;
use termion::scroll;
use termion::input::{TermRead, MouseTerminal};
use termion::raw::IntoRawMode;
use termion::terminal_size;
use termion::screen::*;
use std::io::{Write, stdout, stdin, Stdout};
use crossbeam_channel::unbounded;

use std::fs::File;
use std::io::prelude::*;
use std::io::BufReader;
use termion::raw::RawTerminal;

enum RagerEvent {
    Line(Vec<u8>),
    ScrollDown,
    ScrollUp,
    Quit,
    IncreaseFold,
    DecreaseFold,
    // Resize TODO
}


#[derive(Copy, Clone)]
struct RagerChar(char, bool, bool, ransid::color::Color);

#[derive(Clone)]
struct Buffer(usize, usize, RagerChar, Vec<Vec<RagerChar>>);

impl Buffer {
    fn new(width: usize, height: usize, default: RagerChar) -> Buffer {
        Buffer(
            width,
            height,
            default,
            (0..height).map(|x| vec![default; width]).collect::<Vec<_>>(),
        )
    }

    fn expand_vertical(&mut self) {
        let width = self.0;
        let height = self.1;
        self.1 += 1;

        // for _ in 0..(new_height - height) {
            self.3.push(vec![self.2; width]);
        // }
    }

    fn fold_by_indent(&mut self, indent: usize) {
        let rows = self.3.split_off(0);
        let mut last_index = 0;
        for mut row in rows {
            // Calculate indent
            let row_indent = row.iter().take_while(|&x| x.0 == ' ' || x.0 == '\t').count();
            if row_indent <= indent {
                self.3.push(row);
                last_index = self.3.len() - 1;
            } else {
                // Set underline for previous line.
                self.3.get_mut(last_index).map(|row| {
                    row.iter_mut().for_each(|x| {
                        x.2 = true;
                    });
                });
            }
        }
        self.1 = self.3.len();
    }

    fn set(&mut self, x: usize, y: usize, val: RagerChar) {
        if y >= self.height() {
            for _ in 0..(y - self.height() + 1) {
                self.expand_vertical();
            }
        }
        self.3[y][x] = val;
    }

    fn get(&self, x: usize, y: usize) -> RagerChar {
        self.3[y][x]
    }

    fn width(&self) -> usize {
        self.0
    }

    fn height(&self) -> usize {
        self.1
    }
}

fn main() {
    run().expect("Program error");
}

fn run() -> Result<(), Error> {
    let stdin = stdin();

    // Swap stdin and TTY
    let mut input = if !termion::is_tty(&stdin) {
        // https://stackoverflow.com/a/29694013
        unsafe {
            use std::os::unix::io::*;

            let tty = File::open("/dev/tty").unwrap();

            let stdin_fd = libc::dup(0);

            let ret = File::from_raw_fd(stdin_fd);

            libc::dup2(tty.as_raw_fd(), 0);

            Some(ret)
        }
    } else {
        // TODO load a file from a command line argument here
        None
    };

    let mut screen = MouseTerminal::from(AlternateScreen::from(stdout().into_raw_mode().unwrap()));
    
    write!(screen, "{}", termion::cursor::Hide).unwrap();
    write!(screen, "{}", termion::clear::All).unwrap();
    screen.flush().unwrap();

    // Create the ransid terminal
    let (screen_width, screen_height) = terminal_size().unwrap();
    let screen_width = screen_width as usize;
    let screen_height = screen_height as usize;

    type MyTerminal = MouseTerminal<AlternateScreen<RawTerminal<Stdout>>>;

    let (tx, rx) = unbounded();
    let actor = ::std::thread::spawn({
        // let screen = screen.clone();
        let tx = tx.clone();
        move || {
            let mut console = ransid::Console::new(screen_width, 32767);

            let mut matrix = Buffer::new(screen_width, screen_height, RagerChar(' ', false, false, ransid::color::Color::Ansi(0)));

            let mut original_matrix = None;

            fn write_char(screen: &mut MyTerminal, c: RagerChar, x: usize, y: usize) {
                write!(screen,
                    "{}{}{}{}{}{}",
                    termion::cursor::Goto((x as u16) + 1, (y as u16) + 1),
                    if c.1 { format!("{}", termion::style::Bold) } else { format!("") },
                    if c.2 { format!("{}", termion::style::Underline) } else { format!("") },
                    match c.3 {
                        ransid::color::Color::Ansi(c) => format!("{}", termion::color::Fg(termion::color::AnsiValue(c))),
                        ransid::color::Color::TrueColor(r, g, b) => format!("{}", termion::color::Fg(termion::color::Rgb(r, g, b))),
                    },
                    c.0,
                    termion::style::Reset,
                );
            }

            fn write_row(screen: &mut MyTerminal, buffer: &Buffer, row: usize, dest_row: usize) {
                let matrix_width = buffer.width() as usize;
                for x in 0..matrix_width {
                    write_char(screen, buffer.get(x, row), x, dest_row);
                }
            }

            let redraw_from = |screen: &mut MyTerminal, buffer: &Buffer, row: usize| {
                for y in 0..screen_height {
                    write_row(screen, buffer, row + y, y);
                }
            };

            let update = |screen: &mut MyTerminal, matrix: &mut Buffer, c, x, y, bold, underlined, color| {
                
                let c = RagerChar(c, bold, underlined, color);
                matrix.set(x, y, c);
                
                if y < screen_height {
                    write_char(screen, c, x, y);
                }
            };

            const INDENT_STEP: usize = 2;

            let mut indent: usize = 0;
            let mut scroll: usize = 0;
            while let Ok(event) = rx.recv() {
                match event {
                    // These only work after file's been loaded
                    RagerEvent::IncreaseFold => {
                        if indent == 0 {
                            original_matrix = Some(matrix.clone());
                        } else {
                            matrix = original_matrix.clone().unwrap();
                        }

                        indent += INDENT_STEP;
                        matrix.fold_by_indent(indent);
                        while matrix.height() <= screen_height {
                            matrix.expand_vertical();
                        }
                        scroll = 0;
                        redraw_from(&mut screen, &matrix, 0);
                    }
                    RagerEvent::DecreaseFold => {
                        if indent > 0 {
                            indent -= INDENT_STEP;

                            if indent == 0 {
                                matrix = original_matrix.take().unwrap();
                            } else if indent > 1 {
                                matrix = original_matrix.clone().unwrap();
                                matrix.fold_by_indent(indent);
                                while matrix.height() <= screen_height {
                                    matrix.expand_vertical();
                                }
                            }
                            scroll = 0;
                            redraw_from(&mut screen, &matrix, 0);
                        }
                    }

                    RagerEvent::Line(line) => {
                        let tx = tx.clone();

                        // console.write doesn't need to take ownership of captured
                        // internal values, but it wants to. Here we do some unsafe
                        // casting to make it work.
                        // TODO if we can cast the closure reference instead, that
                        // would be cleaner. Or upstream a patch to ransid.                        
                        unsafe {
                            let screen: &'static mut MyTerminal = ::std::mem::transmute(&mut screen);
                            let matrix: &'static mut Buffer = ::std::mem::transmute(&mut matrix);
                            console.write(&line, move |event| {
                                use ransid::Event;
                                match event {
                                    Event::Char {
                                        x,
                                        y,
                                        c,
                                        bold,
                                        underlined,
                                        color,
                                    } => {
                                        update(screen, matrix, c, x, y, bold, underlined, color);
                                    },

                                    // Ignore all other event types.
                                    _ => {},
                                }
                            });
                        }
                    }
                    RagerEvent::ScrollDown => {
                        if scroll > 0 {
                            write!(screen, "{}", scroll::Down(1)).unwrap();

                            scroll -= 1;
                            write_row(&mut screen, &matrix, scroll, 0);
                        }
                    }
                    RagerEvent::ScrollUp => {
                        if scroll + screen_height < matrix.height() - 1 {
                            write!(screen, "{}", scroll::Up(1)).unwrap();

                            scroll += 1;
                            write_row(&mut screen, &matrix, scroll + screen_height, matrix.height() as usize);
                        }

                    }
                    RagerEvent::Quit => break,
                }
                screen.flush().unwrap();
            }

            write!(screen, "{}", termion::cursor::Show).unwrap();
            screen.flush().unwrap();
        }
    });

    if input.is_some() {
        ::std::thread::spawn({
            let tx = tx.clone();
            let mut input = BufReader::new(input.unwrap());
            let mut buf = vec![];
            move || {
                while let Ok(len) = input.read_until(0xA, &mut buf)  {
                    let _ = tx.send(RagerEvent::Line(buf.clone()));
                    buf.clear();
                }
            }
        });
    }

    for c in stdin.events() {
        match c.unwrap() {
            Event::Key(Key::Char('q')) |
            Event::Key(Key::Ctrl('c')) => break,

            Event::Mouse(MouseEvent::Press(MouseButton::WheelDown, _, _)) | 
            Event::Key(Key::Down) => {
                let _ = tx.send(RagerEvent::ScrollUp);
            }
            
            Event::Mouse(MouseEvent::Press(MouseButton::WheelUp, _, _)) |
            Event::Key(Key::Up) => {
                let _ = tx.send(RagerEvent::ScrollDown);
            }

            Event::Key(Key::Char('>')) => {
                let _ = tx.send(RagerEvent::IncreaseFold);
            }
            Event::Key(Key::Char('<')) => {
                let _ = tx.send(RagerEvent::DecreaseFold);
            }

            c => {
                // eprintln!("got: {:?}", c);
            }
        }
    }

    let _ = tx.send(RagerEvent::Quit);

    actor.join();

    Ok(())
}
