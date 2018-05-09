# rager 🎉

A terminal Pager written in Rust. Like more or less.

Supports any `xterm`-supporting terminal thanks to Termion. Only supports content over stdin (for now). Scroll or up/down keys to move, `q` or Ctrl+C to quit.

```
cargo install rager
cargo build --color=always |& rager
```

![](https://user-images.githubusercontent.com/80639/39799598-cea19382-5332-11e8-9c94-367ec317123f.png)

**TODO:**

* Visually indicate when stdin has terminated.
* Support paging a file via command line argument.
* Support dumping contents to your shell, or switching back.
* Support pausing / resuming output.
* Support follow mode (like `less +F`).
* Add more key shortcuts?
* Windows support?

All contributions welcome. How can rager be useful for you?

## License

MIT or Apache-2.0, at your option.
