#[macro_use]
extern crate structopt;
extern crate termion;
extern crate ransid;
extern crate failure;
extern crate libc;
extern crate crossbeam_channel;

use std::path::PathBuf;
use structopt::StructOpt;
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
use std::io::BufReader;
use termion::raw::RawTerminal;

enum RagerEvent {
    Line(Vec<u8>),
    ScrollDown,
    ScrollUp,
    Quit,
    EndInput,
    Home,
    End,
    PageUp,
    PageDown,
    // Resize TODO
}


#[derive(Copy, Clone)]
struct RagerChar(char, bool, bool, ransid::color::Color);

struct Buffer(usize, usize, RagerChar, Vec<Vec<RagerChar>>);

impl Buffer {
    fn new(width: usize, height: usize, default: RagerChar) -> Buffer {
        Buffer(
            width,
            height,
            default,
            (0..height).map(|_| vec![default; width]).collect::<Vec<_>>(),
        )
    }

    fn expand_vertical(&mut self) {
        let width = self.0;
        self.1 += 1;

        // for _ in 0..(new_height - height) {
            self.3.push(vec![self.2; width]);
        // }
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
#[derive(Debug, StructOpt)]
#[structopt(name = "rager", about = "A pager, like more or less.", author = "")]
struct Opt {
    #[structopt(parse(from_os_str))]
    input: Option<PathBuf>,
}

fn main() {
    let opt = Opt::from_args();
    run(opt.input).expect("Program error");
}

fn run(
    input_file: Option<PathBuf>,
) -> Result<(), Error> {
    let stdin = stdin();

    // Swap stdin and TTY
    let input = if !termion::is_tty(&stdin) {
        // https://stackoverflow.com/a/29694013
        unsafe {
            use std::os::unix::io::*;

            let tty = File::open("/dev/tty").unwrap();

            let stdin_fd = libc::dup(0);

            let ret = File::from_raw_fd(stdin_fd);

            libc::dup2(tty.as_raw_fd(), 0);

            ::std::mem::forget(tty);

            Some(ret)
        }
    } else if let Some(input_file) = input_file {
        // Must have a filename as input.
        let file = File::open(input_file)?;
        Some(file)
    } else {
        // Print error.
        eprintln!("Expected 'rager <input>' or input over stdin.");
        ::std::process::exit(1);
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
        move || {
            let mut console = ransid::Console::new(screen_width, 32767);

            let mut matrix = Buffer::new(screen_width, screen_height, RagerChar(' ', false, false, ransid::color::Color::Ansi(0)));

            fn write_char(screen: &mut MyTerminal, c: RagerChar, x: usize, y: usize) {
                // ::std::thread::sleep(::std::time::Duration::from_millis(1));
                let _ = write!(screen,
                    "{}{}{}{}{}",
                    termion::cursor::Goto((x as u16) + 1, (y as u16) + 1),
                    if c.1 { format!("{}", termion::style::Bold) } else { format!("") },
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
                
                if y < (screen_height as usize) {
                    write_char(screen, c, x, y);
                }
            };

            let mut scroll: usize = 0;
            while let Ok(event) = rx.recv() {
                match event {
                    RagerEvent::Home => {
                        scroll = 0;
                        redraw_from(&mut screen, &mut matrix, scroll);
                    }
                    RagerEvent::End => {
                        scroll = matrix.height() - screen_height;
                        redraw_from(&mut screen, &mut matrix, scroll);
                    }
                    RagerEvent::PageUp => {
                        scroll = if scroll <= screen_height { 0 } else { scroll - screen_height };
                        redraw_from(&mut screen, &mut matrix, scroll);
                    }
                    RagerEvent::PageDown => {
                        let last_row = matrix.height() - screen_height;
                        let next_row = scroll + screen_height;
                        scroll = if next_row >= last_row { last_row } else { next_row };
                        redraw_from(&mut screen, &mut matrix, scroll);
                    }
                    RagerEvent::Line(line) => {
                        unsafe {
                            let screen: &'static mut MyTerminal = ::std::mem::transmute(&mut screen);
                            let matrix: &'static mut Buffer = ::std::mem::transmute(&mut matrix);
                            console.write(&line, move |event| {
                                // TODO this should have a fix right here for the closure to fit into a non-static content, instead of transmuting the value
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
                    RagerEvent::EndInput => {
                        // TODO Draw row
                        // let matrix_width = matrix.width() as usize;
                        // let matrix_height = matrix.height() as usize;
                        // for x in 0..matrix_width {
                        //     write_char(&mut screen, RagerChar('~', false, false, ransid::color::Color::Ansi(15)), x, matrix_height);
                        // }
                    }
                    RagerEvent::ScrollDown => {
                        if scroll > 0 {
                            write!(screen, "{}", scroll::Down(1)).unwrap();

                            scroll -= 1;

                            // Draw row
                            let matrix_width = matrix.width() as usize;
                            for x in 0..matrix_width {
                                write_char(&mut screen, matrix.get(x, scroll), x, 0);
                            }
                        }
                    }
                    RagerEvent::ScrollUp => {
                        if scroll + (screen_height as usize) < matrix.height() - 1 {
                            write!(screen, "{}", scroll::Up(1)).unwrap();

                            scroll += 1;

                            // Draw row
                            let matrix_width = matrix.width() as usize;
                            let matrix_height = matrix.height() as usize;
                            for x in 0..matrix_width {
                                write_char(&mut screen, matrix.get(x, scroll + screen_height as usize), x, matrix_height);
                            }
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
            let mut buf = String::new();
            move || {
                // TODO read_until pegs CPU at 100% without a sleep there
                while let Ok(len) = ::std::io::BufRead::read_line(&mut input, &mut buf) {
                    if len == 0 {
                        break;
                    }

                    let _ = tx.send(RagerEvent::Line(buf.as_bytes().to_owned()));
                    buf.clear();
                }

                let _ = tx.send(RagerEvent::EndInput);
            }
        });
    }

    // tracking gg vim keybind
    let mut pressed_g = 'n';

    for c in stdin.events() {
        match c.unwrap() {
            Event::Key(Key::Char('q')) |
            Event::Key(Key::Ctrl('c')) => break,
            Event::Mouse(MouseEvent::Press(MouseButton::WheelDown, _, _)) | 
            Event::Key(Key::Down) => {
                let _ = tx.send(RagerEvent::ScrollUp);
                pressed_g = 'n';
                // write!(screen.borrow_mut(), "{}", scroll::Up(1)).unwrap();
            }
            Event::Key(Key::Char('j')) => {
                let _ = tx.send(RagerEvent::ScrollUp);
                pressed_g = 'n';
            }
            Event::Mouse(MouseEvent::Press(MouseButton::WheelUp, _, _)) |
            Event::Key(Key::Up) => {
                let _ = tx.send(RagerEvent::ScrollDown);
            }
            Event::Key(Key::Char('k')) => {
                let _ = tx.send(RagerEvent::ScrollDown);
                pressed_g = 'n';
            }
            Event::Key(Key::Home) => {
                let _ = tx.send(RagerEvent::Home);
                pressed_g = 'n';
            }
            Event::Key(Key::Char('g')) => {
                if pressed_g == 'y' {
                    pressed_g = 'n';
                    let _ = tx.send(RagerEvent::Home);
                } else {
                    pressed_g = 'y';
                }
            }
            Event::Key(Key::End) => {
                let _ = tx.send(RagerEvent::End);
                pressed_g = 'n';
            }
            Event::Key(Key::Char('G')) => {
                let _ = tx.send(RagerEvent::End);
                pressed_g = 'n';
            }
            Event::Key(Key::PageUp) => {
                let _ = tx.send(RagerEvent::PageUp);
                pressed_g = 'n';
            }
            Event::Key(Key::Ctrl('B')) => {
                let _ = tx.send(RagerEvent::PageUp);
                pressed_g = 'n';
            }
            Event::Key(Key::PageDown) => {
                let _ = tx.send(RagerEvent::PageDown);
                pressed_g = 'n';
            }
            Event::Key(Key::Ctrl('F')) => {
                let _ = tx.send(RagerEvent::PageDown);
                pressed_g = 'n';
            }
            _ => {},
            // c => {
            //     println!("\r\n$\r\n{:?}", c);
            // }
        }
    }

    let _ = tx.send(RagerEvent::Quit);

    let _ = actor.join();

    Ok(())
}
