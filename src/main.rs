use lazy_static::lazy_static;
use std::ffi::OsStr;
use std::io::Read;
use std::io::Write;
use std::net::UdpSocket;
use std::net::{TcpListener, TcpStream};
use std::thread;
use structopt::StructOpt;
use users::{get_current_gid, get_current_username, get_user_groups};

#[derive(StructOpt, Debug)]
#[structopt(name = "rinput")]
struct Opt {
    /// The port on which to initialize the rinput server
    #[structopt(long, default_value = "44554")]
    port: u16,

    /// The password of server
    #[structopt(long, default_value = "")]
    password: String,
}

lazy_static! {
    static ref CONFIG: Opt = Opt::from_args();
}

fn main() {
    if !is_user_in_input() {
        println!("WARNING! User is not in group `input`! This application may not work!");
    }

    // Init TCP listener
    let listener = match TcpListener::bind(("0.0.0.0", CONFIG.port)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!(
                "Couldn't initialize a TCP listener on port {}: {}",
                CONFIG.port, e
            );
            return;
        }
    };

    // initialize an UDP socket for broadcasting info about this server
    // to everyone in LAN
    thread::spawn(|| broadcast());

    // accept connections
    for stream in listener.incoming() {
        let stream = match stream {
            Ok(s) => s,
            Err(_) => {
                continue;
            }
        };
        thread::spawn(|| handle_client(stream));
    }
}

fn handle_client(mut stream: TcpStream) {
    // Auth
    if !auth(&mut stream) {
        return;
    }
    // means auth successful
    if let Err(e) = stream.write_all(&[0x00]) {
        eprintln!("Error sending successful auth confirmation: {}", e);
        return;
    }

    match stream.peer_addr() {
        Ok(addr) => {
            println!("{} connected!", addr);
        }
        Err(e) => {
            println!("lost connection: {}", e);
            return;
        }
    }

    // init device

    let mut device = uinput::default()
        .unwrap()
        .name("RInput Device")
        .unwrap()
        .event(uinput::event::absolute::Position::X)
        .unwrap()
        .min(-32767) // i16 bounds
        .max(32767)
        .event(uinput::event::absolute::Position::Y)
        .unwrap()
        .min(-32767) // i16 bounds
        .max(32767)
        .event(uinput::event::controller::GamePad::A)
        .unwrap()
        .event(uinput::event::controller::GamePad::B)
        .unwrap()
        .event(uinput::event::controller::GamePad::X)
        .unwrap()
        .event(uinput::event::controller::GamePad::Y)
        .unwrap()
        .event(uinput::event::controller::GamePad::TL)
        .unwrap()
        .event(uinput::event::controller::GamePad::TR)
        .unwrap()
        .event(uinput::event::controller::GamePad::TL2)
        .unwrap()
        .event(uinput::event::controller::GamePad::TR2)
        .unwrap()
        .event(uinput::event::controller::GamePad::Select)
        .unwrap()
        .event(uinput::event::controller::GamePad::Start)
        .unwrap()
        .create()
        .unwrap();

    // disable timeouts
    stream.set_read_timeout(None).unwrap();
    stream.set_write_timeout(None).unwrap();

    let mut bytes = stream.bytes();
    loop {
        let byte = match bytes.next() {
            Some(b) => b,
            None => return,
        };
        let byte = match byte {
            Ok(b) => b,
            Err(e) => {
                eprintln!("Error reading from stream: {}", e);
                return;
            }
        };

        if byte == 0x00 {
            // oh boy
            // analog data
            let x = match take_i16(&mut bytes) {
                Ok(b) => b,
                Err(()) => {
                    eprintln!("Corrupted stream!");
                    return;
                }
            };
            let y = match take_i16(&mut bytes) {
                Ok(b) => b,
                Err(()) => {
                    eprintln!("Corrupted stream!");
                    return;
                }
            };

            device
                .position(&uinput::event::absolute::Position::X, x as i32)
                .unwrap();
            device
                .position(&uinput::event::absolute::Position::Y, y as i32)
                .unwrap();
            device.synchronize().unwrap();
        }
        #[rustfmt::skip]
        match byte as i8 {
             0x00 => {},
             0x01 =>   device.press(&uinput::event::controller::GamePad::A).unwrap(),
            -0x01 => device.release(&uinput::event::controller::GamePad::A).unwrap(),
             0x02 =>   device.press(&uinput::event::controller::GamePad::B).unwrap(),
            -0x02 => device.release(&uinput::event::controller::GamePad::B).unwrap(),
             0x03 =>   device.press(&uinput::event::controller::GamePad::X).unwrap(),
            -0x03 => device.release(&uinput::event::controller::GamePad::X).unwrap(),
             0x04 =>   device.press(&uinput::event::controller::GamePad::Y).unwrap(),
            -0x04 => device.release(&uinput::event::controller::GamePad::Y).unwrap(),
             0x05 =>   device.press(&uinput::event::controller::GamePad::TL).unwrap(),
            -0x05 => device.release(&uinput::event::controller::GamePad::TL).unwrap(),
             0x06 =>   device.press(&uinput::event::controller::GamePad::TL2).unwrap(),
            -0x06 => device.release(&uinput::event::controller::GamePad::TL2).unwrap(),
             0x07 =>   device.press(&uinput::event::controller::GamePad::TR).unwrap(),
            -0x07 => device.release(&uinput::event::controller::GamePad::TR).unwrap(),
             0x08 =>   device.press(&uinput::event::controller::GamePad::TR2).unwrap(),
            -0x08 => device.release(&uinput::event::controller::GamePad::TR2).unwrap(),
             0x09 =>   device.press(&uinput::event::controller::GamePad::Start).unwrap(),
            -0x09 => device.release(&uinput::event::controller::GamePad::Start).unwrap(),
             0x0a =>   device.press(&uinput::event::controller::GamePad::Select).unwrap(),
            -0x0a => device.release(&uinput::event::controller::GamePad::Select).unwrap(),
            _ => {
                eprintln!("Corrupted stream!");
                return;
            },
        };
        device.synchronize().unwrap();
    }
}

fn auth(stream: &mut TcpStream) -> bool {
    let mut string_length: [u8; 1] = [0];
    let mut string = [0; 256];

    // Read password
    if let Err(e) = stream.read_exact(&mut string_length[..]) {
        eprintln!("Error reading from stream: {}", e);
        return false;
    }
    if let Err(e) = stream.read_exact(&mut string[0..(string_length[0] as usize)]) {
        eprintln!("Error reading from stream: {}", e);
        return false;
    }

    let password = match std::str::from_utf8(&string[0..(string_length[0] as usize)]) {
        Ok(u) => u,
        Err(_) => {
            eprintln!("Corrupted stream while auth!");
            return false;
        }
    };

    password == CONFIG.password
}

fn is_user_in_input() -> bool {
    let username = match get_current_username() {
        Some(s) => s,
        None => {
            eprintln!("Can't get current user!");
            return false;
        }
    };

    let groups = match get_user_groups(&username, get_current_gid()) {
        Some(g) => g,
        None => {
            eprintln!("Couldn't get the list of groups the user is in.");
            return false;
        }
    };

    let mut in_input = false;
    for group in groups {
        if group.name() == OsStr::new("input") {
            in_input = true;
        }
    }

    in_input
}

fn take_i16<I: Iterator<Item = Result<u8, std::io::Error>>>(iterator: &mut I) -> Result<i16, ()> {
    match (iterator.next(), iterator.next()) {
        (Some(a), Some(b)) => match (a, b) {
            (Ok(a), Ok(b)) => Ok(i16::from_le_bytes([a, b])),
            _ => Err(()),
        },
        _ => Err(()),
    }
}

fn broadcast() {
    // init UDP socket
    let socket = match UdpSocket::bind("0.0.0.0:0") {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error binding the UDP broadcast socket: {}", e);
            std::process::exit(1);
        }
    };
    if let Err(e) = socket.set_broadcast(true) {
        eprintln!(
            "Failed setting SO_BROADCAST option for the UDP socket: {}",
            e
        );
        std::process::exit(1);
    }

    let mut data: Vec<u8> = Vec::new();

    let hostname = match hostname::get() {
        Ok(h) => h,
        Err(e) => {
            eprintln!("Couldn't get machine's hostname: {}", e);
            std::process::exit(1);
        }
    };
    let hostname = match hostname.into_string() {
        Ok(s) => s,
        Err(_) => {
            eprintln!("Failed converting machine's hostname to a string.");
            std::process::exit(1);
        }
    };
    // hostname for displaying
    data.extend_from_slice(&(hostname.len() as u32).to_le_bytes()[..]);
    data.extend_from_slice(hostname.as_bytes());
    // port
    data.extend_from_slice(&CONFIG.port.to_le_bytes()[..]);

    loop {
        msleep(1000);

        if let Err(e) = socket.send_to(&data[..], "255.255.255.255:44554") {
            eprintln!("Error broadcasting data about server: {}", e);
            continue;
        }
    }
}

fn msleep(ms: u64) {
    thread::sleep(std::time::Duration::from_millis(ms));
}
