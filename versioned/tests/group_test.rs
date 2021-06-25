use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::io::{Cursor, Read, Seek, SeekFrom, Write};
use versioned::group::{DataSource, GroupDeserialize, GroupHeader, UpgradeLatest};
use versioned::{MessageId, Versioned};

/// A header that can be serialized into a fixed-size buffer.
#[derive(Debug, Clone)]
pub struct BasicFixedHeader {
    pub msg_id: u16,
    pub msg_ver: u16,
}

impl BasicFixedHeader {
    pub fn for_msg<T>(_msg: &T) -> Self
    where
        T: Versioned + MessageId,
    {
        BasicFixedHeader {
            msg_id: T::MSG_ID,
            msg_ver: T::VER,
        }
    }

    pub fn new(msg_id: u16, msg_ver: u16) -> Self {
        BasicFixedHeader { msg_id, msg_ver }
    }

    /// Deserialize a header from a `Read` stream.
    pub fn deserialize_from(r: &mut impl Read) -> Result<Self, MyGroupError> {
        let msg_id = r.read_u16::<BigEndian>()?;
        let msg_ver = r.read_u16::<BigEndian>()?;
        Ok(BasicFixedHeader { msg_id, msg_ver })
    }

    /// Serialize a header into a `Write` stream.
    pub fn serialize_into(&self, w: &mut impl Write) -> Result<(), MyGroupError> {
        w.write_u16::<BigEndian>(self.msg_id)?;
        w.write_u16::<BigEndian>(self.msg_ver)?;
        Ok(())
    }
}

// Maybe this should be derived?
impl GroupHeader for BasicFixedHeader {
    fn msg_id(&self) -> u16 {
        self.msg_id
    }

    fn msg_ver(&self) -> u16 {
        self.msg_ver
    }
}

enum FooBase {}

#[derive(Debug, PartialEq, Versioned, Serialize, Deserialize)]
struct FooV1 {
    foo: u32,
}

type Foo = FooV1;

enum BarBase {}

#[derive(Debug, PartialEq, Versioned, Serialize, Deserialize)]
struct BarV1 {
    bar: u64,
}

type Bar = BarV1;

// This should be derived
#[derive(Debug, PartialEq)]
enum MyGroup1 {
    Foo(Foo),
    Bar(Bar),
}

// This should be derived
impl MessageId for FooV1 {
    const MSG_ID: u16 = 0x70;
}

// This should be derived
impl MessageId for BarV1 {
    const MSG_ID: u16 = 0x71;
}

// This should be derived
impl UpgradeLatest for Foo {
    fn upgrade_latest<Src>(src: &mut Src, ver: u16) -> Result<Self, Src::Error>
    where
        Src: DataSource,
    {
        match ver {
            1 => {
                let msg = src.read_message::<FooV1>()?;
                Ok(msg)
            }
            _ => Err(src.unknown_version::<Foo>(ver)),
        }
    }
}

// This should be derived
impl UpgradeLatest for Bar {
    fn upgrade_latest<Src>(src: &mut Src, ver: u16) -> Result<Self, Src::Error>
    where
        Src: DataSource,
    {
        match ver {
            1 => {
                let msg = src.read_message::<BarV1>()?;
                Ok(msg)
            }
            _ => Err(src.unknown_version::<Bar>(ver)),
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct MyGroupError;

impl From<serde_cbor::Error> for MyGroupError {
    fn from(_: serde_cbor::Error) -> Self {
        MyGroupError
    }
}

impl From<std::io::Error> for MyGroupError {
    fn from(_: std::io::Error) -> Self {
        MyGroupError
    }
}

struct MyStream {
    reader: Box<dyn Read>,
}

// This impl is user-defined.
impl DataSource for MyStream {
    type Error = MyGroupError;
    type Header = BasicFixedHeader;

    fn read_header(&mut self) -> Result<Self::Header, Self::Error> {
        BasicFixedHeader::deserialize_from(&mut self.reader)
    }

    fn read_message<T>(&mut self) -> Result<T, Self::Error>
    where
        T: DeserializeOwned,
    {
        let msg: T = serde_cbor::from_reader(&mut self.reader)?;
        Ok(msg)
    }
}

// This should be derived
impl GroupDeserialize for MyGroup1 {
    fn read_message<Src>(src: &mut Src) -> Result<Self, Src::Error>
    where
        Src: DataSource,
    {
        let header: Src::Header = src.read_header()?;
        match header.msg_id() {
            Foo::MSG_ID => {
                let msg = Foo::upgrade_latest(src, header.msg_ver())?;
                Ok(MyGroup1::Foo(msg))
            }
            Bar::MSG_ID => {
                let msg = Bar::upgrade_latest(src, header.msg_ver())?;
                Ok(MyGroup1::Bar(msg))
            }
            _ => {
                // Call the user-supplied error fn
                Err(src.unknown_message(header.msg_id()))
            }
        }
    }
    fn expect_message<Src, T>(src: &mut Src) -> Result<T, Src::Error>
    where
        Src: DataSource,
        T: MessageId + UpgradeLatest,
    {
        let header: Src::Header = src.read_header()?;
        if header.msg_id() == T::MSG_ID {
            T::upgrade_latest(src, header.msg_ver())
        } else {
            // Call the user-supplied error fn
            Err(src.unexpected_message::<T>(header.msg_id()))
        }
    }
}

#[test]
fn test_group() {
    let mut cursor = Cursor::new(Vec::<u8>::new());

    let my_foo = Foo { foo: 1234 };
    let header = BasicFixedHeader::for_msg(&my_foo);

    // FIXME: add a DataSink trait for writing
    header.serialize_into(&mut cursor).unwrap();
    serde_cbor::to_writer(&mut cursor, &my_foo).unwrap();

    // Reset the cursor so we can do some reading.
    cursor.seek(SeekFrom::Start(0)).unwrap();

    let mut my_stream = MyStream {
        reader: Box::new(cursor),
    };

    let message = MyGroup1::read_message(&mut my_stream).unwrap();
    assert_eq!(message, MyGroup1::Foo(Foo { foo: 1234 }));
}
