# rager 🎉

A terminal Pager written in Rust. Like more or less.

Supports any `xterm`-supporting terminal thanks to Termion. Only supports content over stdin (for now). Scroll or up/down keys to move, `q` or Ctrl+C to quit.

```
cargo install rager
cargo build --color=always |& rager
```

![](https://user-images.githubusercontent.com/80639/39799598-cea19382-5332-11e8-9c94-367ec317123f.png)

## License

MIT or Apache-2.0, at your option.
