use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use thrussh::server;
use thrussh::server::{Auth, Session};
use thrussh::{ChannelId, CryptoVec};
use thrussh_keys::key;

#[derive(Clone)]
pub struct Server {
    pub client_pubkey: Arc<thrussh_keys::key::PublicKey>,
    pub clients: Arc<Mutex<HashMap<(usize, ChannelId), thrussh::server::Handle>>>,
    pub id: usize,
}

impl server::Server for Server {
    type Handler = Self;

    fn new(&mut self, _: Option<std::net::SocketAddr>) -> Self {
        let s = self.clone();
        self.id += 1;
        s
    }
}

impl server::Handler for Server {
    type Error = color_eyre::eyre::Report;
    type FutureAuth = futures::future::Ready<Result<(Self, Auth), Self::Error>>;
    type FutureUnit = futures::future::Ready<Result<(Self, Session), Self::Error>>;
    type FutureBool = futures::future::Ready<Result<(Self, Session, bool), Self::Error>>;

    fn finished_auth(self, auth: Auth) -> Self::FutureAuth {
        futures::future::ready(Ok((self, auth)))
    }

    fn finished_bool(self, b: bool, s: Session) -> Self::FutureBool {
        futures::future::ready(Ok((self, s, b)))
    }

    fn finished(self, s: Session) -> Self::FutureUnit {
        futures::future::ready(Ok((self, s)))
    }

    fn channel_open_session(self, channel: ChannelId, session: Session) -> Self::FutureUnit {
        {
            let mut clients = self.clients.lock().unwrap();
            clients.insert((self.id, channel), session.handle());
        }
        self.finished(session)
    }

    fn auth_publickey(self, _user: &str, _key: &key::PublicKey) -> Self::FutureAuth {
        self.finished_auth(server::Auth::Reject)
    }

    fn auth_password(self, user: &str, password: &str) -> Self::FutureAuth {
        if user == "root" && password == "hackmepls" {
            self.finished_auth(server::Auth::Accept)
        } else {
            self.finished_auth(server::Auth::Reject)
        }
    }

    fn data(self, channel: ChannelId, data: &[u8], mut session: Session) -> Self::FutureUnit {
        {
            // let mut clients = self.clients.lock().unwrap();
            // for ((id, channel), ref mut s) in clients.iter_mut() {
            //     if *id != self.id {
            //         s.data(*channel, CryptoVec::from_slice(data));
            //     }
            // }
        }
        session.data(channel, CryptoVec::from_slice(data));
        self.finished(session)
    }
}
