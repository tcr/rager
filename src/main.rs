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
// use std::cell::RefCell;
use std::io::BufReader;
// use std::sync::Arc;
use termion::raw::RawTerminal;

// fn write_alt_screen_msg<W: Write>(screen: &mut W) {
//     write!(screen, "{}{}Welcome to the alternate screen.{}Press '1' to switch to the main screen or '2' to switch to the alternate screen.{}Press 'q' to exit (and switch back to the main screen).",
//            termion::clear::All,
//            termion::cursor::Goto(1, 1),
//            termion::cursor::Goto(1, 3),
//            termion::cursor::Goto(1, 4)).unwrap();
// }

enum RagerEvent {
    Line(Vec<u8>),
    // Char(char, usize, usize, bool, bool, ransid::color::Color),
    ScrollDown,
    ScrollUp,
    Quit,
    // Resize
}


#[derive(Copy, Clone)]
struct RagerChar(char, bool, bool, ransid::color::Color);

struct Vec2D<T>(usize, usize, T, Vec<Vec<T>>);

impl<T: Copy> Vec2D<T> {
    fn new(width: usize, height: usize, default: T) -> Vec2D<T> {
        Vec2D(
            width,
            height,
            default,
            (0..height).map(|x| vec![default; width]).collect::<Vec<_>>(),
        )
    }

    fn expand_vertical(&mut self) {
        let width = self.0;
        let height = self.1;
        // let new_height = ((height as f32) * 1.4) as usize;;
        // self.1 = new_height;
        self.1 += 1;

        // for _ in 0..(new_height - height) {
            self.3.push(vec![self.2; width]);
        // }
    }

    fn set(&mut self, x: usize, y: usize, val: T) {
        if y >= self.height() {
            for _ in 0..(y - self.height() + 1) {
                self.expand_vertical();
            }
        }
        self.3[y][x] = val;
    }

    fn get(&self, x: usize, y: usize) -> T {
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
        None
    };

    let mut screen = MouseTerminal::from(AlternateScreen::from(stdout().into_raw_mode().unwrap()));
    write!(screen, "{}", termion::cursor::Hide).unwrap();
    // write_alt_screen_msg(&mut screen);

    write!(screen, "{}", termion::clear::All).unwrap();

    screen.flush().unwrap();

    // Load the file
    // let mut file = File::open("test")?;
    // let mut contents = vec![];
    // file.read_to_end(&mut contents)?;

    // Create the ransid terminal
    let (width, height) = terminal_size().unwrap();

    type MyTerminal = MouseTerminal<AlternateScreen<RawTerminal<Stdout>>>;

    let (tx, rx) = unbounded();
    let actor = ::std::thread::spawn({
        // let screen = screen.clone();
        let tx = tx.clone();
        move || {
            let mut console = ransid::Console::new(width as usize, 32767);

            let mut matrix = Vec2D::new(width as usize, height as usize, RagerChar(' ', false, false, ransid::color::Color::Ansi(0)));

            fn write_char(screen: &mut MyTerminal, c: RagerChar, x: usize, y: usize) {
                // ::std::thread::sleep(::std::time::Duration::from_millis(1));
                write!(screen,
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

            let update = |screen: &mut MyTerminal, matrix: &mut Vec2D<RagerChar>, c, x, y, bold, underlined, color| {
                
                let c = RagerChar(c, bold, underlined, color);
                matrix.set(x, y, c);
                
                if y < (height as usize) {
                    write_char(screen, c, x, y);
                }
            };

            let mut scroll: usize = 0;
            while let Ok(event) = rx.recv() {
                // let screen = Arc::new(RefCell::new(screen));

                // TODO
                // screen.borrow_mut().flush().unwrap();
                match event {
                    RagerEvent::Line(line) => {
                        let tx = tx.clone();
                        unsafe {
                            let screen: &'static mut MyTerminal = ::std::mem::transmute(&mut screen);
                            let matrix: &'static mut Vec2D<RagerChar> = ::std::mem::transmute(&mut matrix);
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
                    RagerEvent::ScrollDown => {
                        if scroll > 0 {
                            write!(screen, "{}", scroll::Down(1)).unwrap();

                            scroll -= 1;

                            // Draw row
                            let matrix_width = matrix.width() as usize;
                            let matrix_height = matrix.height() as usize;
                            for x in 0..matrix_width {
                                write_char(&mut screen, matrix.get(x, scroll), x, 0);
                            }
                        }
                    }
                    RagerEvent::ScrollUp => {
                        if scroll + (height as usize) < matrix.height() - 1 {
                            write!(screen, "{}", scroll::Up(1)).unwrap();

                            scroll += 1;

                            // Draw row
                            let matrix_width = matrix.width() as usize;
                            let matrix_height = matrix.height() as usize;
                            for x in 0..matrix_width {
                                write_char(&mut screen, matrix.get(x, scroll + height as usize), x, matrix_height);
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
                // write!(screen.borrow_mut(), "{}", scroll::Up(1)).unwrap();
            }
            Event::Mouse(MouseEvent::Press(MouseButton::WheelUp, _, _)) |
            Event::Key(Key::Up) => {
                let _ = tx.send(RagerEvent::ScrollDown);
                // write!(screen.borrow_mut(), "{}", scroll::Down(1)).unwrap();
            }
            c => {
                // write!(screen.borrow_mut(), "\r\n{:?}", c).unwrap();
            }
        }
        // screen.borrow_mut().flush().unwrap();
    }

    let _ = tx.send(RagerEvent::Quit);

    actor.join();

    Ok(())
}
