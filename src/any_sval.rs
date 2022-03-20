use hex::encode;
use opentelemetry::sdk::log::Any;
use std::{borrow::Cow, collections::BTreeMap};
use sval::stream::{Arguments, OwnedStream, Result, Stack, Stream};

pub struct SvalAny {
    slot: Option<Slot>,
    stack: Stack,
}

enum Slot {
    Val(Any),
    Map {
        map: BTreeMap<Cow<'static, str>, Any>,
        key: Option<Any>,
    },
    List(Vec<Any>),
}

impl SvalAny {
    pub fn new() -> SvalAny {
        Self {
            slot: None,
            stack: Stack::new(),
        }
    }
}

impl Default for SvalAny {
    fn default() -> Self {
        Self::new()
    }
}

macro_rules! trivial_stream {
    ($name:ident, $ty:ty) => {
        fn $name(&mut self, val: $ty) -> Result {
            self.set(val.into())
        }
    };
}

impl SvalAny {
    pub fn stream() -> OwnedStream<Self> {
        OwnedStream::new(SvalAny::default())
    }

    pub fn value(&mut self) -> Option<Any> {
        let old = std::mem::replace(&mut self.slot, None);
        if let Some(Slot::Val(v)) = old {
            Some(v)
        } else {
            None
        }
    }

    fn set(&mut self, val: Any) -> Result {
        let pos = self.stack.current();
        self.stack.primitive()?;

        if pos.is_key() {
            if let Some(Slot::Map { ref mut key, .. }) = self.slot {
                *key = Some(val);
            }
        } else if pos.is_value() {
            if let Some(Slot::Map {
                ref mut map,
                ref mut key,
            }) = self.slot
            {
                map.insert(any_to_string(&key.take().unwrap()).into(), val);
            }
        } else if pos.is_elem() {
            if let Some(Slot::List(ref mut list)) = self.slot {
                list.push(val);
            }
        } else {
            self.slot = Some(Slot::Val(val));
        }
        Ok(())
    }
}

impl Stream for SvalAny {
    fn fmt(&mut self, args: Arguments) -> Result {
        let val = args.to_string().into();
        self.set(val)
    }

    fn u64(&mut self, v: u64) -> Result {
        self.i64(v as i64)
    }

    fn char(&mut self, v: char) -> Result {
        self.str(&v.to_string())
    }

    trivial_stream!(i64, i64);
    trivial_stream!(f64, f64);
    trivial_stream!(bool, bool);
    trivial_stream!(str, &str);

    fn map_begin(&mut self, _len: Option<usize>) -> Result {
        self.stack.map_begin()?;
        self.slot = Some(Slot::Map {
            map: BTreeMap::new(),
            key: None,
        });
        Ok(())
    }

    fn map_key(&mut self) -> Result {
        self.stack.map_key().map(|_| ())
    }

    fn map_value(&mut self) -> Result {
        self.stack.map_value().map(|_| ())
    }

    fn map_end(&mut self) -> Result {
        self.stack.map_end()?;
        let old = std::mem::replace(&mut self.slot, None);
        if let Some(Slot::Map { map, .. }) = old {
            self.slot = Some(Slot::Val(Any::Map(map)));
        }
        Ok(())
    }

    fn seq_begin(&mut self, len: Option<usize>) -> Result {
        self.stack.seq_begin()?;
        self.slot = Some(Slot::List(Vec::with_capacity(len.unwrap_or_default())));
        Ok(())
    }

    fn seq_elem(&mut self) -> Result {
        self.stack.seq_elem().map(|_| ())
    }

    fn seq_end(&mut self) -> Result {
        self.stack.seq_end()?;
        let old = std::mem::replace(&mut self.slot, None);
        if let Some(Slot::List(list)) = old {
            self.slot = Some(Slot::Val(Any::ListAny(list)))
        }
        Ok(())
    }
}

fn any_to_string(val: &Any) -> String {
    match val {
        Any::Int(v) => format!("{}", v),
        Any::Double(v) => format!("{}", v),
        Any::String(v) => format!("{}", v),
        Any::Boolean(v) => format!("{}", v),
        Any::Bytes(bytes) => encode(bytes),
        Any::ListAny(list) => {
            let mut builder = String::from("[");
            for val in list.iter() {
                builder.push_str(&any_to_string(val));
                builder.push(' ')
            }
            builder.push(']');
            builder
        }
        Any::Map(map) => {
            let mut builder = String::from("{ ");
            for (key, val) in map.iter() {
                builder.push_str(key);
                builder.push(':');
                builder.push_str(&any_to_string(val));
            }
            builder.push_str(" }");
            builder
        }
    }
}
