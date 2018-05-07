#![feature(io)]

extern crate libc;
extern crate pancurses;
extern crate crossbeam_channel;

use std::env;
use std::fs::File;
use std::io::prelude::*;

use crossbeam_channel::unbounded;
use pancurses::{ALL_MOUSE_EVENTS, endwin, getmouse, cbreak, initscr, mousemask, half_delay, Input, noecho, newpad, Window};

struct DisplayPad {
    pad: Window,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
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
        };
        pad.clear();
        pad
    }

    fn clear(&mut self) {
        self.pad.bkgd(' ');
    }

    fn append(&mut self, value: &[u8]) {
        if self.y > self.height {
            // ignore
            return;
        }
        for c in value {
            match *c {
                b'\n' => {
                    self.y += 1;
                    self.x = 1;
                }
                b'\r' => {
                    self.x = 1;
                }
                c => {
                    self.pad.mvinsch(self.y, self.x, c as char);
                    self.x += 1;
                    if self.x >= self.width {
                        self.x = 1;
                        self.y += 1;
                    }
                }
            }
        }
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

    let (tx, rx) = unbounded::<u8>();
    ::std::thread::spawn(move || {
        let buf = ::std::io::BufReader::new(&mut stdin);
        // println!("start\r");
        for c in buf.bytes() {
            if let Ok(c) = c {
                tx.send(c);
            }
        }
    });

    let mut value: isize = 0;

    // os.dup2(f.fileno(), 0);

    let mut scroll_pos = 0;
    // pad.append(&contents);
    window.nodelay(true);
    loop {
        pad.pad.prefresh(scroll_pos,1,0,0,max_y-1,max_x);
        
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
                    pad.append(&[c]);
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
