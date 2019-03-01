#![feature(try_from)]

use scroll;

use scroll::ctx::{TryFromCtx, TryIntoCtx, MeasureWith};
use std::convert::{TryInto, TryFrom, From, Into};
use scroll::{Pread, Pwrite};
use std::marker::PhantomData;

// Wrapper around a data type, passes a slice of a specific size to that type.
#[derive(Clone)]
struct LengthData<Data, Length>(Data, PhantomData<Length>);

trait Endian {
    fn get_val() -> scroll::Endian;
}

#[derive(Clone)]
struct LittleEndian;
#[derive(Clone)]
struct BigEndian;

impl Endian for LittleEndian {
    fn get_val() -> scroll::Endian {
        scroll::Endian::Little
    }
}

impl Endian for BigEndian {
    fn get_val() -> scroll::Endian {
        scroll::Endian::Big
    }
}

// It would be nice if I could just use a () as context rather than scroll::Endian because I don't
// use it, but the derive macro doesn't allow for that. Should I pass on the endian ctx though?
impl <'a, Data, Length> TryFromCtx<'a, scroll::Endian> for LengthData<Data, Length>
        // Data is something we can read
    where Data: TryFromCtx<'a> + 'a,
        // Length is something that we can use as usize
          Length: TryInto<usize>
        // Length is something that we can read
                + TryFromCtx<'a, scroll::Endian> + 'a,
        // We can convert errors into the error we return.
          scroll::Error: From<<Data as scroll::ctx::TryFromCtx<'a>>::Error>
                       + From<<Length as scroll::ctx::TryFromCtx<'a, scroll::Endian>>::Error>,
        // We can... convert from our error into others? don't know why this was necessary.
          <Data as TryFromCtx<'a>>::Error: From<scroll::Error>,
          <Length as TryFromCtx<'a, scroll::Endian>>::Error: From<scroll::Error>,
        // be able to call unwrap.
          <Length as TryInto<usize>>::Error: std::error::Error {

    type Error = scroll::Error;
    fn try_from_ctx(src: &'a [u8], _ctx: scroll::Endian) -> Result<(Self, usize), Self::Error> {
        let mut read = 0;
        let length: usize = src.gread::<Length>(&mut read)?.try_into().unwrap();

        Ok((LengthData((&src[read..read+length]).pread(0)?, PhantomData), read))
    }
}

impl<'a, Data, Length> TryIntoCtx<scroll::Endian> for &LengthData<Data, Length>
    where Data: MeasureWith<scroll::Endian> + TryIntoCtx<scroll::Endian> + Clone,
        Length: TryIntoCtx<scroll::Endian>,
        usize: TryInto<Length>,
        <usize as TryInto<Length>>::Error: std::fmt::Debug,
        <Length as TryIntoCtx<scroll::Endian>>::Error: From<scroll::Error>,
        scroll::Error: From<<Length as TryIntoCtx<scroll::Endian>>::Error>,
        <Data as TryIntoCtx<scroll::Endian>>::Error: std::convert::From<scroll::Error>,
        scroll::Error: From<<Data as TryIntoCtx<scroll::Endian>>::Error> {
    type Error = scroll::Error;
    fn try_into_ctx(self, dest: &mut [u8], ctx: scroll::Endian) -> Result<usize, Self::Error> {
        let mut offset = 0;
        dest.gwrite_with::<Length>(self.0.measure_with(&ctx).try_into().unwrap(), &mut offset, ctx)?;
        dest.gwrite_with(self.0.clone(), &mut offset, ctx)?;
        Ok(offset)
    }
}

impl<Ctx, Data, Length> MeasureWith<Ctx> for LengthData<Data, Length>
    where Data: MeasureWith<Ctx>,
        Length: MeasureWith<Ctx>,
        <usize as std::convert::TryInto<Length>>::Error : std::fmt::Debug,
        usize: TryInto<Length> {
    #[inline]
    fn measure_with(&self, ctx: &Ctx) -> usize {
        let data_len = self.0.measure_with(ctx);
        let length: Length = data_len.try_into().unwrap();
        length.measure_with(ctx) + data_len
    }
}

// Reads an entire buffer as a UTF-16 string of specified endian. maybe use StrCtx from scroll in the future?
#[derive(Clone)]
struct UTF16<E>(String, PhantomData<E>);

impl<'a, E> TryFromCtx<'a> for UTF16<E> where E: Endian  + 'a {
    type Error = scroll::Error;
    fn try_from_ctx(src: &'a [u8], _ctx: ()) -> Result<(Self, usize), Self::Error> {
        if src.len() % 2 != 0 {
            return Err(scroll::Error::Custom("Length of utf-16 string is not a multiple of 2!".to_owned()));
        }

        let mut offset = 0;
        let mut data = vec![0u16; src.len()/2];
        src.gread_inout_with(&mut offset, &mut data, E::get_val())?;
        Ok((UTF16(String::from_utf16_lossy(&data).into(), PhantomData), src.len()))
    }
}

impl<'a, E> TryIntoCtx<scroll::Endian> for UTF16<E> where E: Endian + 'a {
    type Error = scroll::Error;
    fn try_into_ctx(self, dest: &mut [u8], _ctx: scroll::Endian) -> Result<usize, Self::Error> {
        let mut size = 0;
        self.0.encode_utf16().try_for_each(|char| dest.gwrite_with(char, &mut size, E::get_val()).map(|_| ()))?;
        Ok(size)
    }
}

impl<Ctx, E> MeasureWith<Ctx> for UTF16<E> where E: Endian {
    #[inline]
    fn measure_with(&self, _ctx: &Ctx) -> usize {
        self.0.encode_utf16().count() * 2
    }
}

impl<Length, Endian> From<LengthData<UTF16<Endian>, Length>> for String {
    fn from(src: LengthData<UTF16<Endian>, Length>) -> Self {
        (src.0).0
    }
}

// Wrapper around a type specifying a constant endian, move to const generics when available.
// can probably become CtxWrapper with const generics
#[derive(Clone)]
struct EndianWrapper<T, E>(T, PhantomData<E>);

impl<'a, T, E> TryFromCtx<'a, scroll::Endian> for EndianWrapper<T, E>
    where T: TryFromCtx<'a, scroll::Endian>,
        <T as TryFromCtx<'a, scroll::Endian>>::Error: From<scroll::Error>,
        scroll::Error: From<<T as TryFromCtx<'a, scroll::Endian>>::Error>,
        E: Endian + 'a {
    type Error = scroll::Error;
    fn try_from_ctx(src: &'a [u8], _ctx: scroll::Endian) -> Result<(Self, usize), Self::Error> {
        let mut read = 0;
        Ok((EndianWrapper(src.gread_with(&mut read, E::get_val())?, PhantomData), read))
    }
}

impl<'a, T, E> TryIntoCtx<scroll::Endian> for &EndianWrapper<T, E>
    where T: TryIntoCtx<scroll::Endian> + Clone + 'a,
        <T as TryIntoCtx<scroll::Endian>>::Error: From<scroll::Error>,
        scroll::Error: From<<T as TryIntoCtx<scroll::Endian>>::Error>,
        E: Endian + 'a {
    type Error = scroll::Error;
    fn try_into_ctx(self, dest: &mut [u8], _ctx: scroll::Endian) -> Result<usize, Self::Error> {
        let mut size = 0;
        dest.gwrite_with(self.0.clone(), &mut size, E::get_val())?;
        Ok(size)
    }
}

impl<'a, T, E> TryIntoCtx<scroll::Endian> for EndianWrapper<T, E>
    where T: TryIntoCtx<scroll::Endian> + 'a,
        <T as TryIntoCtx<scroll::Endian>>::Error: From<scroll::Error>,
        scroll::Error: From<<T as TryIntoCtx<scroll::Endian>>::Error>,
        E: Endian + 'a {
    type Error = scroll::Error;
    fn try_into_ctx(self, dest: &mut [u8], _ctx: scroll::Endian) -> Result<usize, Self::Error> {
        let mut size = 0;
        dest.gwrite_with(self.0, &mut size, E::get_val())?;
        Ok(size)
    }
}

impl<Ctx, T, E> MeasureWith<Ctx> for EndianWrapper<T, E> where T: MeasureWith<scroll::Endian>, E: Endian {
    #[inline]
    fn measure_with(&self, _ctx: &Ctx) -> usize {
        self.0.measure_with(&E::get_val())
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
        src.0.try_into()
    }
}

impl<Endian> TryFrom<usize> for EndianWrapper<u16, Endian> {
    type Error = <u16 as TryFrom<usize>>::Error;
    fn try_from(src: usize) -> Result<Self, Self::Error> {
        Ok(EndianWrapper(src.try_into()?, PhantomData))
    }
}

// Now that *that* is all out of the way... usage!

#[derive(Pread, Pwrite)]
struct Example {
    big: EndianWrapper<u16, BigEndian>,
    little: EndianWrapper<u16, LittleEndian>,
    var: LengthData<UTF16<LittleEndian>, EndianWrapper<u16, BigEndian>>
}

fn main() {
    let src = [0u8, 42, 42, 0, 0, 10, 0x48, 0, 0x65, 0, 0x6c, 0, 0x6c, 0, 0x6f, 0];
    let mut dest = vec![0; src.len()];
    let example: Example = src.pread(0).unwrap();
    let big: u16 = example.big.clone().into();
    let little: u16 = example.little.clone().into();
    let var: String = example.var.clone().into();
    dest.pwrite(example, 0).unwrap();

    println!("Example {{ big: {}, little: {}, var: {} }}", big, little, var);

    assert_eq!(big, 42u16);
    assert_eq!(little, 42u16);
    assert_eq!(var, "Hello".to_owned());
    assert_eq!(src.as_ref(), dest.as_slice());
}
