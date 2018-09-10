#![feature(try_from)]

extern crate scroll;
extern crate byteorder;

use scroll::ctx::TryFromCtx;
use std::convert::{TryInto, TryFrom, From, Into};
use scroll::Pread;
use byteorder::{ByteOrder, LE, BE};
use std::marker::PhantomData;

// Wrapper around a data type, passes a slice of a specific size to that type.
struct LengthData<Data, Length>(Data, PhantomData<Length>);

// It would be nice if I could just use a () as context rather than scroll::Endian because I don't
// use it, but the derive macro doesn't allow for that.
impl <'a, Data, Length> TryFromCtx<'a, scroll::Endian> for LengthData<Data, Length>
        // Data is something we can read
    where Data: TryFromCtx<'a, Size = usize> + 'a,
        // Length is something that we can use as usize
          Length: TryInto<usize>
        // Length is something that we can read
                + TryFromCtx<'a, scroll::Endian, Size = usize> + 'a,
        // We can convert errors into the error we return.
          scroll::Error: From<<Data as scroll::ctx::TryFromCtx<'a>>::Error>
                       + From<<Length as scroll::ctx::TryFromCtx<'a, scroll::Endian>>::Error>,
        // We can... convert from our error into others? don't know why this was necessary.
          <Data as TryFromCtx<'a>>::Error: From<scroll::Error>,
          <Length as TryFromCtx<'a, scroll::Endian>>::Error: From<scroll::Error>,
        // be able to call unwrap.
          <Length as TryInto<usize>>::Error: std::error::Error {

    type Error = scroll::Error;
    type Size = usize;
    fn try_from_ctx(src: &'a [u8], _ctx: scroll::Endian) -> Result<(Self, Self::Size), Self::Error> {
        let read = &mut 0usize;
        let length: usize = src.gread::<Length>(read)?.try_into().unwrap();

        Ok((LengthData((&src[*read..*read+length]).pread(0)?, PhantomData), *read))
    }
}

// Reads an entire buffer as a UTF-16 string of specified endian. maybe use StrCtx from scroll in the future?
struct UTF16<Endian>(String, PhantomData<Endian>);

impl<'a, Endian> TryFromCtx<'a> for UTF16<Endian> where Endian: ByteOrder + 'a {
    type Error = scroll::Error;
    type Size = usize;
    fn try_from_ctx(src: &'a [u8], _ctx: ()) -> Result<(Self, Self::Size), Self::Error> {
        if src.len() % 2 != 0 {
            return Err(scroll::Error::Custom("Length of utf-16 string is not a multiple of 2!".to_owned()));
        }

        let mut data = vec![0u16; src.len()/2];
        Endian::read_u16_into(src, &mut data); // used instead of scroll::Endian because less boilerplate.
        Ok((UTF16(String::from_utf16_lossy(&data).into(), PhantomData), src.len()))
    }
}

impl<Length, Endian> From<LengthData<UTF16<Endian>, Length>> for String {
    fn from(src: LengthData<UTF16<Endian>, Length>) -> Self {
        (src.0).0
    }
}

// Wrapper around an integer specifying endian.
struct EndianWrapper<T, Endian>(T, PhantomData<Endian>);

// This can't be fully generic because ByteOrder only provides individual read_X() methods.
impl<'a, Endian> TryFromCtx<'a, scroll::Endian> for EndianWrapper<u16, Endian> where Endian: ByteOrder + 'a {
    type Error = scroll::Error;
    type Size = usize;
    fn try_from_ctx(src: &'a [u8], _ctx: scroll::Endian) -> Result<(Self, Self::Size), Self::Error> {
        let size = std::mem::size_of::<u16>();
        if src.len() < size {
            Err(scroll::Error::TooBig{size, len: src.len()})
        } else {
            Ok((EndianWrapper(Endian::read_u16(src), PhantomData), size))
        }
    }
}

// This can't be fully generic either because of limitations and possible trait impl conflicts.
impl<Endian> From<EndianWrapper<u16, Endian>> for u16 {
    fn from(src: EndianWrapper<Self, Endian>) -> Self {
        src.0
    }
}

// for the sake of usage as a Length in LengthData
impl<Endian> TryFrom<EndianWrapper<u16, Endian>> for usize {
    type Error = <usize as TryFrom<u16>>::Error;
    fn try_from(src: EndianWrapper<u16, Endian>) -> Result<Self, Self::Error> {
        TryFrom::try_from(src.0)
    }
}

// Now that *that* is all out of the way... usage!

#[derive(Pread)]
struct Example {
    big: EndianWrapper<u16, BE>,
    little: EndianWrapper<u16, LE>,
    var: LengthData<UTF16<LE>, EndianWrapper<u16, BE>>
}

fn main() {
    let src = [0u8, 42, 42, 0, 0, 10, 0x48, 0, 0x65, 0, 0x6c, 0, 0x6c, 0, 0x6f, 0];
    let example: Example = src.pread(0).unwrap();
    let big: u16 = example.big.into();
    let little: u16 = example.little.into();
    let var: String = example.var.into();

    println!("Example {{ big: {}, little: {}, var: {} }}", big, little, var);

    assert_eq!(big, 42u16);
    assert_eq!(little, 42u16);
    assert_eq!(var, "Hello".to_owned());
}
