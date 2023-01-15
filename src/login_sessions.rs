use std::collections::LinkedList;

use tokio::net::TcpStream;

pub struct LoginSessions {
    list: LinkedList<LoginSessionData>,
}

impl LoginSessions {
    pub fn new() -> Self {
        Self {
            list: LinkedList::new(),
        }
    }
}

pub struct LoginSessionData {
    login: [u8; 16],
    acc_id: u32,
    service_d: u32,
    client_addr: u32,
    client_port: u16,
    serv_ip: u32,

    char_name: [u8; 15],
    login_socket: TcpStream,
    login_lobbydata_socket: TcpStream,
    login_lobbyview_socket: TcpStream,
    login_lobbyconf_socket: TcpStream,

    just_created_new_char: bool,
}
