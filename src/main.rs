#![feature(io)]

extern crate libc;
extern crate pancurses;
extern crate ncurses;
extern crate crossbeam_channel;
extern crate vte;

use std::env;
use std::fs::File;
use std::io::prelude::*;

use crossbeam_channel::unbounded;
use pancurses::{ALL_MOUSE_EVENTS, A_BOLD, A_COLOR, COLOR_BLACK, COLOR_RED, A_NORMAL, endwin, getmouse, cbreak, init_pair, initscr, mousemask, half_delay, Input, noecho, newpad, Window};

struct DisplayPad {
    pad: Window,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    vte_parser: Option<vte::Parser>,
}

impl DisplayPad {
    fn new(width: i32) -> DisplayPad {
        let height = 32767;
        let mut pad = DisplayPad {
            pad: newpad(height, width),
            x: 1,
            y: 0,
            width,
            height,
            vte_parser: Some(vte::Parser::new()),
        };
        init_pair(0, COLOR_BLACK, COLOR_RED);
        pad.clear();
        pad
    }

    fn clear(&mut self) {
        self.pad.bkgd(' ');
    }

    fn append(&mut self, value: &str) {
        if self.y > self.height {
            // ignore
            return;
        }

        // eprint!("---#> {:?} ", value.len());
        let mut parser = self.vte_parser.take().unwrap();
        for b in value.bytes() {
            parser.advance(self, b);
        }

        self.vte_parser = Some(parser)
    }
}

impl vte::Perform for DisplayPad {
    fn print(&mut self, c: char) {
        let mut a = String::new();
        a.push(c);
        // eprint!("{:?} ", a);
        self.pad.printw(&a);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            b'\n' | b'\r' | 0x8 => {
                let mut a = String::new();
                a.push(byte as char);
                self.pad.printw(&a);
            }
            _ => {
                eprint!("#E{}# ", byte);
            }
        }
    }
    
    fn hook(&mut self, params: &[i64], intermediates: &[u8], ignore: bool) {
        eprint!("#H# ");
    }
    
    fn put(&mut self, byte: u8) {
        // let mut a = String::new();
        // self.pad.printw(&a);
        eprint!("#P# ");
    }
    
    fn unhook(&mut self) {
        eprint!("#U# ");
    }

    fn osc_dispatch(&mut self, params: &[&[u8]]) {
        eprint!("#O# ");
    }
    
    fn csi_dispatch(
        &mut self, 
        params: &[i64], 
        intermediates: &[u8], 
        ignore: bool, 
        c: char
    ) { 
        // eprint!("#C {} {:?} {:?}# ", c, params, intermediates);
        if c == 'm' {
            match params[0] {
                0 => {
                }
                32 => {
                    // self.pad.chgat(-1, A_BOLD, 0);
                    // self.pad.attron(A_BOLD);
                }
                _ => {},
            }
        }
    }

    fn esc_dispatch(
        &mut self, 
        params: &[i64], 
        intermediates: &[u8], 
        ignore: bool, 
        byte: u8
    ) { 
        eprint!("#S# ");
    }
}

fn main() {
    let window = initscr();

    window.keypad(true); // Set keypad mode
    mousemask(ALL_MOUSE_EVENTS, std::ptr::null_mut()); // Listen to all mouse events

    // window.printw("Click in the terminal, press q to exit\n");
    noecho();
    cbreak();
    window.refresh();

    let max_x = window.get_max_x();
    let max_y = window.get_max_y();

    pancurses::mouseinterval(0);

    // let mut f = File::open("ok").expect("file not found");

    // let mut contents = String::new();
    // f.read_to_string(&mut contents)
    //     .expect("something went wrong reading the file");

    let mut pad = DisplayPad::new(max_x);

    // unsafe {
    //     use std::os::unix::io::AsRawFd;
    //     let f = File::open("/dev/tty").unwrap();
    //     libc::dup2(f.as_raw_fd(), 0);
    // }

    // https://stackoverflow.com/a/29694013
    let mut stdin = unsafe {
        use std::os::unix::io::*;

        let tty = File::open("/dev/tty").unwrap();

        let stdin_fd = libc::dup(0);

        let ret = File::from_raw_fd(stdin_fd);

        libc::dup2(tty.as_raw_fd(), 0);

        ret
    };

    let (tx, rx) = unbounded::<String>();
    ::std::thread::spawn(move || {
        let buf = ::std::io::BufReader::new(&mut stdin);
        // println!("start\r");
        for c in buf.lines() {
            if let Ok(c) = c {
                tx.send(format!("{}", c));
            }
        }
    });

    let mut value: isize = 0;

    // os.dup2(f.fileno(), 0);

    let mut scroll_pos = 0;
    // pad.append(&contents);
    window.nodelay(true);
    loop {
        pad.pad.prefresh(scroll_pos,0,0,0,max_y-1,max_x);
        
        match window.getch() {
            Some(Input::KeyMouse) => {
                if let Ok(mouse_event) = getmouse() {
                    if mouse_event.bstate & (1 << 7) != 0 {
                        if scroll_pos < 4000 {
                            scroll_pos += 1;
                        }
                    }
                    if mouse_event.bstate & (1 << 19) != 0 {
                        if scroll_pos > 0 {
                            scroll_pos -= 1;
                        }
                    }
                    // window.mvprintw(1, 0, &format!("value: {}", value));
                };
            }
            Some(Input::Character(x)) => {
                if x == 'q' {
                    break;
                } else {
                    println!("x {:?}", x);
                }
            }
            // a => {
            //     println!("a {:?}", a);
            // }
            None => {
                if let Ok(c) = rx.try_recv() {
                    pad.append(&c);
                    pad.append("\n");
                    window.refresh();
                } else {
                    ::std::thread::sleep(::std::time::Duration::from_millis(10));
                }
            }
            _ => (),
        }
    }
    endwin();
}
