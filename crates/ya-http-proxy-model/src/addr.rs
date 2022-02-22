use std::fmt::{Display, Formatter};
use std::net::SocketAddr;
use std::ops::Add;

use serde::{de, Deserialize, Serialize};

use crate::deser::one_or_many::OneOrManyVisitor;

/// Socket address collection wrapper
#[derive(Default, Clone, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct Addresses(pub Vec<SocketAddr>);

impl Addresses {
    pub fn to_vec(&self) -> Vec<SocketAddr> {
        self.0.clone()
    }
}

impl<'de> Deserialize<'de> for Addresses {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let mut addrs: Vec<SocketAddr> =
            deserializer.deserialize_any(OneOrManyVisitor::<SocketAddr>::default())?;

        if addrs.is_empty() {
            return Err(de::Error::custom("empty sequence"));
        }

        addrs.sort();
        addrs.dedup();

        Ok(Addresses(addrs.into_iter().collect()))
    }
}

impl Add for Addresses {
    type Output = Self;

    fn add(mut self, rhs: Self) -> Self {
        self.0.extend(rhs.0);
        self.0.sort();
        self.0.dedup();
        self
    }
}

impl From<SocketAddr> for Addresses {
    #[inline]
    fn from(addr: SocketAddr) -> Self {
        Self(vec![addr])
    }
}

impl From<Vec<SocketAddr>> for Addresses {
    #[inline]
    fn from(vec: Vec<SocketAddr>) -> Self {
        Self(vec)
    }
}

impl Display for Addresses {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "[")?;
        for (i, addr) in self.0.iter().enumerate() {
            if i != 0 {
                write!(f, ", ")?;
            }
            addr.fmt(f)?;
        }
        write!(f, "]")
    }
}
