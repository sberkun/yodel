
use core::slice::SlicePattern;
use std::error::Error;
use std::collections::HashMap;
use std::io::{self, Read, Write};
use mio::net::{TcpListener, TcpStream, SocketAddr};
use mio::{Events, Interest, Poll, Token};

const SERVER_ADDR:&str = "127.0.0.1:5001";


struct ClientHandler {
    inb: Vec<u8>,
    outb: Vec<u8>,
    connection: TcpStream,
    address: std::net::SocketAddr
}

fn bytes_to_u32(ar: &[u8]) -> u32 {
    ((ar[0] as u32) <<  0) +
    ((ar[1] as u32) <<  8) +
    ((ar[2] as u32) << 16) +
    ((ar[3] as u32) << 24)
}


impl ClientHandler {
    fn pop_message(&mut self) -> Option<(Vec<u8>, Vec<u8>)> {
        if self.inb.len() < 8 {
            return None
        }
        let len1 = bytes_to_u32(&self.inb[0..4]);
        let len2 = bytes_to_u32(&self.inb[4..8]);
        let total_len = 8 + (len1 + len2) as usize;
        if self.inb.len() < total_len {
            return None
        }
        let target = self.inb[0..4].to_vec();
        let (m_view, r_view) = self.inb.split_at(total_len);
        let message = m_view.to_vec();
        let mut remainder = r_view.to_vec();
        self.inb.clear();
        self.inb.append(&mut remainder);
        Some((target, message))
    }

    fn push_message(&mut self, message: &Vec<u8>) {
        self.outb.extend_from_slice(message.as_slice())
    }

    fn try_write(&mut self) -> io::Result<()> {
        if self.outb.len() == 0 {
            return Ok(())
        }
        match self.connection.write(self.outb.as_slice()) {
            Ok(n) => {
                self.outb.drain(0..n);
                Ok(())
            },
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(()),
            Err(e) => Err(e)
        }
    }

    // returns true if connection closed
    fn try_read(&mut self) -> io::Result<bool> {
        let mut buf = [0 as u8; 4096];
        loop {
            match self.connection.read(&mut buf) {
                Ok(0) => return Ok(true),
                Ok(n) => self.inb.extend_from_slice(&buf[0..n]),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => return Ok(false),
                Err(e) => return Err(e)
            }
        }
    }
}



fn main() -> io::Result<()> {
    // Create a poll instance.
    let mut poll = Poll::new()?;
    // Create storage for events.
    let mut events = Events::with_capacity(1024);

    // Setup the server socket.
    let mut server = TcpListener::bind(SERVER_ADDR.parse().unwrap())?;
    // Start listening for incoming connections.
    poll.registry()
        .register(&mut server, Token(0), Interest::READABLE)?;

    // a token of value x points to index x - 1
    let mut free_tokens = Vec::<Token>::new();
    let mut client_handlers = Vec::<ClientHandler>::new();

    println!("Yodel server started at {}", SERVER_ADDR);
    // Setup the client socket.
    //let mut client = TcpStream::connect(addr)?;
    // Register the socket.
    //poll.registry()
    //    .register(&mut client, CLIENT, Interest::READABLE | Interest::WRITABLE)?;

    // Start an event loop.
    loop {
        // Poll Mio for events, blocking until we get an event.
        poll.poll(&mut events, None)?;
        for event in events.iter() {
            match event.token() {
                Token(0) => loop {
                    // If this is an event for the server, it means connection(s)
                    // are ready to be accepted.
                    match server.accept() {
                        Ok((connection, address)) => {
                            let ch = ClientHandler {
                                inb: Vec::new(),
                                outb: Vec::new(),
                                connection,
                                address
                            };
                            println!("Accepted connected from {}", address);
                            let token = if let Some(token) = free_tokens.pop() {
                                client_handlers[token.0 - 1] = ch;
                                token
                            } else {
                                client_handlers.push(ch);
                                Token(client_handlers.len())
                            };
                            poll.registry().register(
                                &mut client_handlers[token.0].connection,
                                token,
                                Interest::READABLE)?;
                        },
                        Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                            break;
                        },
                        Err(e) => {
                            return Err(e); // fatal error
                        }
                    }
                }
                token => {
                    let ch = &mut client_handlers[token.0 - 1];
                    if event.is_writable() {
                        // We can (likely) write to the socket without blocking.
                        ch.try_write()?;
                        if ch.outb.len() == 0 {
                            poll.registry().reregister(
                                &mut ch.connection, token, Interest::READABLE)?;
                        }
                    }
                    if event.is_readable() {
                        // We can (likely) read from the socket without blocking.
                        let connection_closed = ch.try_read()?;
                        if connection_closed {
                            poll.registry().deregister(&mut ch.connection)?;

                        }
                        //TODO: if EOF, deregister as readible
                        //TODO: if EOF, free client handler
                        //TODO: if EOF

                        //TODO: register all forwarded thingies as writable
                    }
                }
            }
        }
    }
}