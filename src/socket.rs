use ipnetwork::Ipv4Network;
use log::LevelFilter;
use rlimit::setrlimit;
use spdlog::{prelude::*, sink::Sink};
use spdlog::{Logger, LoggerBuilder};
use std::net::Ipv4Addr;
use std::ops::Deref;
use std::str::{FromStr, Split};
use std::time::Duration;

use crate::{error::ServerError, settings::Settings};
use mlua::{FromLua, Value};

struct SocketBuilder {
    enable_ip_rules: bool,
    stall_time: Duration,
    access_order: AccessOrder,
    access_allow: Vec<Ipv4Network>,
    access_deny: Vec<Ipv4Network>,
    connect_count: usize,
    connect_interval: Duration,
    connect_lockout: Duration,
    logger: Logger,
}

impl SocketBuilder {
    fn new() -> Self {
        Self {
            enable_ip_rules: true,
            stall_time: Duration::from_secs(60),
            access_order: AccessOrder::DenyAllow,
            access_allow: Vec::new(),
            access_deny: Vec::new(),
            connect_count: 10,
            connect_interval: Duration::from_secs(3),
            connect_lockout: Duration::from_secs(10 * 60),
            // empty logger
            logger: Logger::builder().build().unwrap(),
        }
    }

    fn build(self) -> Socket {
        Socket {
            enable_ip_rules: self.enable_ip_rules,
            stall_time: self.stall_time,
            access_order: self.access_order,
            access_allow: self.access_allow,
            access_deny: self.access_deny,
            connect_count: self.connect_count,
            connect_interval: self.connect_interval,
            connect_lockout: self.connect_lockout,
            logger: self.logger,
        }
    }

    fn ip_rules(mut self, enable: bool) -> Self {
        self.enable_ip_rules = enable;
        self
    }

    fn stall_time(mut self, n: Duration) -> Self {
        self.stall_time = n;
        self
    }

    fn access_order(mut self, n: AccessOrder) -> Self {
        self.access_order = n;
        self
    }

    fn access_allow(mut self, n: Vec<Ipv4Network>) -> Self {
        self.access_allow = n;
        self
    }

    fn access_deny(mut self, n: Vec<Ipv4Network>) -> Self {
        self.access_deny = n;
        self
    }

    fn connect_count(mut self, n: usize) -> Self {
        self.connect_count = n;
        self
    }

    fn connect_interval(mut self, n: Duration) -> Self {
        self.connect_interval = n;
        self
    }

    fn connect_lockout(mut self, n: Duration) -> Self {
        self.connect_lockout = n;
        self
    }

    fn logger(mut self, n: Logger) -> Self {
        self.logger = n;
        self
    }
}

pub struct Socket {
    enable_ip_rules: bool,
    stall_time: Duration,
    access_order: AccessOrder,
    access_allow: Vec<Ipv4Network>,
    access_deny: Vec<Ipv4Network>,
    connect_count: usize,
    connect_interval: Duration,
    connect_lockout: Duration,
    logger: Logger,
}

impl Socket {
    pub fn new(
        enable_ip_rules: bool,
        stall_time: Duration,
        access_order: AccessOrder,
        access_allow: Vec<Ipv4Network>,
        access_deny: Vec<Ipv4Network>,
        connect_count: usize,
        connect_interval: Duration,
        connect_lockout: Duration,
        logger: Logger,
    ) -> Socket {
        Socket {
            enable_ip_rules,
            stall_time,
            access_order,
            access_allow,
            access_deny,
            connect_count,
            connect_interval,
            connect_lockout,
            logger,
        }
    }
}

impl Socket {
    fn builder() -> SocketBuilder {
        SocketBuilder::new()
    }
}

pub enum AccessOrder {
    DenyAllow,
    AllowDeny,
    MutualFailure,
}

#[derive(PartialEq, Eq)]
enum AccessKind {
    Allow,
    Deny,
}

impl AccessOrder {
    fn from_str(ordering: &str) -> AccessOrder {
        if ordering == "deny,allow" {
            AccessOrder::DenyAllow
        } else if ordering == "allow,deny" {
            AccessOrder::AllowDeny
        } else if ordering == "mutual-failure" {
            AccessOrder::MutualFailure
        } else {
            AccessOrder::DenyAllow
        }
    }
}

fn socket_init_tcp(
    mut log_builder: LoggerBuilder,
    settings: &Settings,
) -> Result<Socket, ServerError> {
    let enable_logger = settings.try_get::<bool>("network.TCP_DEBUG")?
        || settings.try_get::<bool>("logging.DEBUG_SOCKETS")?;

    let logger = log_builder
        .level_filter(if enable_logger {
            spdlog::LevelFilter::All
        } else {
            spdlog::LevelFilter::Off
        })
        .name("tcp")
        .build()
        .map_err(ServerError::LoggerError)?;

    Ok(Socket::builder()
        .stall_time(Duration::from_secs(
            settings.try_get::<u64>("network.TCP_STALL_TIME")?,
        ))
        .ip_rules(settings.try_get::<bool>("network.TCP_ENABLE_IP_RULES")?)
        .access_order(AccessOrder::from_str(
            &settings.try_get::<String>("network.TCP_ORDER")?,
        ))
        .access_allow(load_access_list(
            AccessKind::Allow,
            &settings.try_get::<String>("network.TCP_ALLOW")?,
            &logger,
        ))
        .access_deny(load_access_list(
            AccessKind::Deny,
            &settings.try_get::<String>("network.TCP_DENY")?,
            &logger,
        ))
        .connect_count(settings.try_get::<usize>("network.TCP_CONNECT_COUNT")?)
        .connect_interval(Duration::from_millis(
            settings.try_get::<u64>("network.TCP_CONNECT_INTERVAL")?,
        ))
        .connect_lockout(Duration::from_millis(
            settings.try_get::<u64>("network.TCP_CONNECT_LOCKOUT")?,
        ))
        .logger(logger)
        .build())
}

fn create_session(
    fd: u64,
    recv: impl Fn(u64) -> u64,
    send: impl Fn(u64) -> u64,
    parse: impl Fn(u64) -> u64,
) -> () {
}

fn load_access_list(
    kind: AccessKind,
    access_list: &str,
    logger: &Logger,
) -> Vec<Ipv4Network> {
    let kind_str = if kind == AccessKind::Allow {
        "allow"
    } else {
        "deny"
    };

    info!(logger: logger, "Loading {} access list...", kind_str);

    let result: Vec<Ipv4Network> = access_list
        .split(',')
        .filter(|x| x.deref() != "")
        .filter_map(|x| access_ipmask(x, logger))
        .collect();

    info!(
        logger: logger,
        "Size of {} access list: {}",
        kind_str,
        result.len()
    );

    result
}

fn access_ipmask(s: &str, logger: &Logger) -> Option<Ipv4Network> {
    if s == "all" {
        return Ipv4Network::new(Ipv4Addr::new(0, 0, 0, 0), 0).ok();
    }

    let result = Ipv4Network::from_str(s);

    match result {
        Ok(network) => info!(
            logger: logger,
            "access_ipmask: Loaded IP:{} mask:{}",
            network.ip(),
            network.mask()
        ),
        Err(_) => error!(
            logger: logger,
            "get_access_list: Invalid ip or ip range '{}'!", s
        ),
    }

    result.ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn logger() -> Logger {
        Logger::builder().build().unwrap()
    }

    #[test]
    fn it_builds_socket() {
        Socket::builder().build();
    }

    #[test]
    fn it_parses_access_list() {
        assert_eq!(
            load_access_list(
                AccessKind::Allow,
                "127.0.0.1,192.168.0.0/16",
                &logger()
            ),
            vec!(
                access_ipmask("127.0.0.1", &logger()).unwrap(),
                access_ipmask("192.168.0.0/16", &logger()).unwrap(),
            )
        );
    }

    #[test]
    fn it_parses_ip_range() {
        assert_eq!(
            access_ipmask("all", &logger()),
            Some(Ipv4Network::new(Ipv4Addr::new(0, 0, 0, 0), 0).unwrap())
        );

        assert!(access_ipmask("127.0.0.1", &logger()).is_some());
        assert_eq!(
            access_ipmask("127.0.0.1", &logger()).unwrap().mask(),
            Ipv4Addr::new(255, 255, 255, 255)
        );

        assert!(access_ipmask("192.168.0.0/16", &logger()).is_some());
        assert!(access_ipmask("10.0.0.0/255.0.0.0", &logger()).is_some());
    }
}
