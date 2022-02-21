#![allow(unused)]

pub mod duration {
    pub mod ms {
        //! (de)serialize `std::time::Duration` from / to u64 milliseconds
        use std::fmt;
        use std::time::Duration;

        use serde::{de, ser};

        pub struct Visitor;

        impl<'de> de::Visitor<'de> for Visitor {
            type Value = Duration;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                write!(formatter, "unsigned number of milliseconds")
            }

            fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(Duration::from_millis(v))
            }
        }

        pub fn deserialize<'de, D>(d: D) -> Result<Duration, D::Error>
        where
            D: de::Deserializer<'de>,
        {
            d.deserialize_u64(Visitor)
        }

        pub fn serialize<S>(d: &Duration, s: S) -> Result<S::Ok, S::Error>
        where
            S: ser::Serializer,
        {
            s.serialize_u64(d.as_millis() as u64)
        }
    }

    pub mod opt_ms {
        //! (de)serialize `Option<std::time::Duration>` from / to u64 milliseconds option
        use std::fmt;
        use std::time::Duration;

        use super::ms::Visitor as MsVisitor;
        use serde::{de, ser};

        pub struct Visitor;

        impl<'de> de::Visitor<'de> for Visitor {
            type Value = Option<Duration>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                write!(formatter, "optional unsigned number of milliseconds")
            }

            fn visit_none<E>(self) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(None)
            }

            fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
            where
                D: de::Deserializer<'de>,
            {
                Ok(Some(deserializer.deserialize_u64(MsVisitor)?))
            }

            fn visit_unit<E>(self) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(None)
            }
        }

        pub fn deserialize<'de, D>(d: D) -> Result<Option<Duration>, D::Error>
        where
            D: de::Deserializer<'de>,
        {
            d.deserialize_option(Visitor)
        }

        pub fn serialize<S>(o: &Option<Duration>, s: S) -> Result<S::Ok, S::Error>
        where
            S: ser::Serializer,
        {
            match *o {
                Some(d) => s.serialize_u64(d.as_millis() as u64),
                None => s.serialize_none(),
            }
        }
    }

    pub mod double_opt_ms {
        //! (de)serialize `Option<Option<std::time::Duration>>`
        //! from / to u64 milliseconds double-option
        use std::fmt;
        use std::time::Duration;

        use super::opt_ms::Visitor as OptVisitor;
        use serde::{de, ser};

        pub struct Visitor;

        impl<'de> de::Visitor<'de> for Visitor {
            type Value = Option<Option<Duration>>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                write!(formatter, "double-optional unsigned number of milliseconds")
            }

            fn visit_none<E>(self) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(Some(None))
            }

            fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
            where
                D: de::Deserializer<'de>,
            {
                Ok(Some(deserializer.deserialize_option(OptVisitor)?))
            }

            fn visit_unit<E>(self) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(Some(None))
            }
        }

        pub fn deserialize<'de, D>(d: D) -> Result<Option<Option<Duration>>, D::Error>
        where
            D: de::Deserializer<'de>,
        {
            d.deserialize_option(Visitor)
        }

        pub fn serialize<S>(oo: &Option<Option<Duration>>, s: S) -> Result<S::Ok, S::Error>
        where
            S: ser::Serializer,
        {
            match oo {
                Some(o) => match *o {
                    Some(d) => s.serialize_u64(d.as_millis() as u64),
                    None => s.serialize_some(&None::<Duration>),
                },
                None => s.serialize_none(),
            }
        }
    }
}

pub mod double_opt {
    //! (de)serialize `Option<Option<T>>`
    use std::fmt;
    use std::marker::PhantomData;

    use serde::{de, ser};

    pub struct Visitor<T> {
        _inner: PhantomData<T>,
    }

    impl<'de, T> de::Visitor<'de> for Visitor<T>
    where
        T: de::Deserialize<'de>,
    {
        type Value = Option<Option<T>>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            write!(formatter, "a double option value")
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(Some(None))
        }

        fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: de::Deserializer<'de>,
        {
            de::Deserialize::deserialize(deserializer).map(Some)
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(Some(None))
        }
    }

    pub fn deserialize<'de, T: de::Deserialize<'de> + std::fmt::Debug, D>(
        d: D,
    ) -> Result<Option<Option<T>>, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        d.deserialize_option(Visitor {
            _inner: PhantomData,
        })
    }

    pub fn serialize<S, T>(oo: &Option<Option<T>>, s: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
        T: ser::Serialize,
    {
        match oo {
            Some(o) => s.serialize_some(o),
            None => s.serialize_none(),
        }
    }
}

pub mod uri {
    use std::fmt;

    use http::uri::Uri;
    use serde::{de, ser};

    pub struct Visitor;

    impl<'de> de::Visitor<'de> for Visitor {
        type Value = Uri;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            write!(formatter, "expected an URL string")
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            v.parse().map_err(de::Error::custom)
        }
    }

    pub fn deserialize<'de, D>(d: D) -> Result<Uri, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        d.deserialize_str(Visitor)
    }

    pub fn serialize<S>(v: &Uri, s: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        s.serialize_str(&v.to_string())
    }
}

pub mod one_or_many {
    use std::fmt;
    use std::marker::PhantomData;
    use std::str::FromStr;

    use serde::{de, Deserialize};

    pub struct OneOrManyVisitor<T> {
        marker: PhantomData<T>,
    }

    impl<T> Default for OneOrManyVisitor<T> {
        fn default() -> Self {
            Self {
                marker: PhantomData,
            }
        }
    }

    impl<'de, T> de::Visitor<'de> for OneOrManyVisitor<T>
    where
        T: FromStr + Deserialize<'de>,
    {
        type Value = Vec<T>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a single `SocketAddr`s or a sequence thereof")
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            let addr = v
                .parse()
                .map_err(|_| de::Error::invalid_value(de::Unexpected::Str(v), &self))?;
            Ok(vec![addr])
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: de::SeqAccess<'de>,
        {
            let mut vec = Vec::new();
            while let Some(addr) = seq.next_element()? {
                vec.push(addr);
            }
            Ok(vec)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;
    use std::str::FromStr;
    use std::time::Duration;

    use crate::deser::one_or_many::OneOrManyVisitor;
    use serde::{de, Deserialize, Serialize};

    #[derive(Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
    struct SerdeStruct {
        #[serde(skip_serializing_if = "Option::is_none")]
        #[serde(default, with = "super::double_opt")]
        pub number: Option<Option<u32>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[serde(default, with = "super::duration::opt_ms")]
        pub duration: Option<Duration>,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[serde(default, with = "super::duration::double_opt_ms")]
        pub duration_double: Option<Option<Duration>>,
    }

    #[derive(Debug, Default, PartialEq, Eq, Serialize)]
    struct SerdeProperty {
        pub inner: Vec<SocketAddr>,
    }

    impl<'de> Deserialize<'de> for SerdeProperty {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: de::Deserializer<'de>,
        {
            let mut addrs: Vec<SocketAddr> =
                deserializer.deserialize_any(OneOrManyVisitor::<SocketAddr>::default())?;
            addrs.sort();
            addrs.dedup();

            Ok(SerdeProperty { inner: addrs })
        }
    }

    struct SerdeHelper {
        ser: String,
    }

    impl SerdeHelper {
        pub fn new<T: Serialize>(t: &T) -> Self
        where
            T: Serialize,
        {
            let ser = serde_json::to_string(t).unwrap();
            Self { ser }
        }

        pub fn de<'de, T>(&'de self) -> T
        where
            T: Deserialize<'de>,
        {
            serde_json::from_str(&self.ser).unwrap()
        }
    }

    #[test]
    fn double_option() {
        let mut st = SerdeStruct::default();
        st.number = Some(Some(1024));
        assert_eq!(st, SerdeHelper::new(&st).de());
        st.number = Some(None);
        assert_eq!(st, SerdeHelper::new(&st).de());
        st.number = None;
        assert_eq!(st, SerdeHelper::new(&st).de());
    }

    #[test]
    fn option_duration() {
        let mut st = SerdeStruct::default();
        st.duration = Some(Duration::from_secs(1024));
        assert_eq!(st, SerdeHelper::new(&st).de());
        st.duration = None;
        assert_eq!(st, SerdeHelper::new(&st).de());
    }

    #[test]
    fn double_option_duration() {
        let mut st = SerdeStruct::default();
        st.duration_double = Some(Some(Duration::from_secs(1024)));
        assert_eq!(st, SerdeHelper::new(&st).de());
        st.duration_double = Some(None);
        assert_eq!(st, SerdeHelper::new(&st).de());
        st.duration_double = None;
        assert_eq!(st, SerdeHelper::new(&st).de());
    }

    #[test]
    fn one_or_many() {
        let sp: SerdeProperty = serde_json::from_str(r#""0.0.0.0:0""#).unwrap();
        assert_eq!(
            *sp.inner.get(0).unwrap(),
            SocketAddr::from_str("0.0.0.0:0").unwrap()
        );

        let sp: SerdeProperty = serde_json::from_str(r#"["0.0.0.0:0", "1.1.1.1:1"]"#).unwrap();
        assert_eq!(
            *sp.inner.get(0).unwrap(),
            SocketAddr::from_str("0.0.0.0:0").unwrap()
        );
        assert_eq!(
            *sp.inner.get(1).unwrap(),
            SocketAddr::from_str("1.1.1.1:1").unwrap()
        );
    }
}
