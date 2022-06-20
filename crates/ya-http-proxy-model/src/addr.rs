use std::collections::HashSet;
use std::fmt::{Display, Formatter};
use std::net::SocketAddr;
use std::ops::{Add, AddAssign};

use serde::{de, Deserialize, Serialize};

use crate::deser::one_or_many::OneOrManyVisitor;

/// Socket address collection wrapper
#[derive(Default, Clone, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct Addresses(Vec<SocketAddr>);

impl Addresses {
    pub fn new(addrs: Vec<SocketAddr>) -> Self {
        Addresses::default() + addrs
    }

    pub fn ports(&self) -> HashSet<u16> {
        self.0.iter().map(|a| a.port()).collect()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    #[inline]
    pub fn to_vec(&self) -> Vec<SocketAddr> {
        self.0.clone()
    }
}

impl<'de> Deserialize<'de> for Addresses {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let addrs = Addresses::new(
            deserializer.deserialize_any(OneOrManyVisitor::<SocketAddr>::default())?,
        );
        if addrs.is_empty() {
            return Err(de::Error::custom("empty sequence"));
        }
        Ok(addrs)
    }
}

impl<I: IntoIterator<Item = SocketAddr>> Add<I> for Addresses {
    type Output = Self;

    #[inline]
    fn add(mut self, rhs: I) -> Self::Output {
        self.add_assign(rhs);
        self
    }
}

impl<I: IntoIterator<Item = SocketAddr>> AddAssign<I> for Addresses {
    fn add_assign(&mut self, rhs: I) {
        self.0.extend(rhs);
        self.0.sort();
        self.0.dedup();
    }
}

impl From<SocketAddr> for Addresses {
    #[inline]
    fn from(addr: SocketAddr) -> Self {
        Self(vec![addr])
    }
}

impl IntoIterator for Addresses {
    type Item = SocketAddr;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
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
