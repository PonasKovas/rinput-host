# RInput

This is the host repository, [client here](https://github.com/PonasKovas/rinput-client).

Written in Rust, this application lets you use your mobile phone as a wireless controller on your PC (Linux only).
Does not depend on X or Wayland, because it uses the `/dev/uinput` virtual file (just like
[ydotool](https://github.com/ReimuNotMoe/ydotool), check it out by the way, it's a cool app) and for this reason
you'll need to run it as root or add your user to group `input`.

## Usage

```
rinput 0.1.0

USAGE:
    rinput [OPTIONS]

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

OPTIONS:
        --password <password>    The password of server [default: ]
        --port <port>            The port on which to initialize the rinput server [default: 44554]
```

## Contributions

All contributions are very welcome.

I'd add Windows support (and maybe MacOS?), if I were a masochist, because, seriously, Windows suck, sorry.
But if you're capable of and willing to do that, that'd be great!
