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

trait IntoInner {
    type Inner;
    fn into_inner(self) -> Self::Inner;
}

trait FromInner {
    type Inner;
    fn from_inner(inner: Self::Inner) -> Self;
}

#[derive(Clone)]
struct TermWrapper<T>(T);

impl<'a, T, Ctx> TryFromCtx<'a, Ctx> for TermWrapper<T>
    where Ctx: Copy,
        T: TryFromCtx<'a, Ctx> + 'a {

    type Error = <T as TryFromCtx<'a, Ctx>>::Error;
    fn try_from_ctx(src: &'a [u8], ctx: Ctx) -> Result<(Self, usize), Self::Error> {
        T::try_from_ctx(src, ctx).map(|(val, size)| (TermWrapper(val), size))
    }
}

impl<'a, T, Ctx> TryIntoCtx<Ctx> for TermWrapper<T>
    where Ctx: Copy,
        T: TryIntoCtx<Ctx> {
    type Error = <T as TryIntoCtx<Ctx>>::Error;
    fn try_into_ctx(self, dest: &mut [u8], ctx: Ctx) -> Result<usize, Self::Error> {
        self.0.try_into_ctx(dest, ctx)
    }
}

impl<T> IntoInner for TermWrapper<T> {
    type Inner = T;
    fn into_inner(self) -> T {
        self.0
    }
}

impl<T> FromInner for TermWrapper<T> {
    type Inner = T;
    fn from_inner(inner: Self::Inner) -> Self {
        TermWrapper(inner)
    }
}

type D<T> = TermWrapper<T>;

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

impl<Data, Length> IntoInner for LengthData<Data, Length> where Data: IntoInner {
    type Inner = <Data as IntoInner>::Inner;
    fn into_inner(self) -> Self::Inner {
        self.0.into_inner()
    }
}

impl<Data, Length> FromInner for LengthData<Data, Length> where Data: FromInner {
    type Inner = <Data as FromInner>::Inner;
    fn from_inner(inner: Self::Inner) -> Self {
        LengthData(Data::from_inner(inner), PhantomData)
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

impl<E> IntoInner for UTF16<E> {
    type Inner = String;
    fn into_inner(self) -> String {
        self.0
    }
}

impl<E> FromInner for UTF16<E> {
    type Inner = String;
    fn from_inner(inner: String) -> Self {
        UTF16(inner, PhantomData)
    }
}

// Wrapper around a type specifying a constant endian, move to const generics when available.
// can probably become CtxWrapper with const generics
#[derive(Clone)]
struct EndianWrapper<T, E>(T, PhantomData<E>);

impl<'a, T, E> TryFromCtx<'a, scroll::Endian> for EndianWrapper<T, E>
    where T: TryFromCtx<'a, scroll::Endian>,
        E: Endian + 'a {
    type Error = <T as TryFromCtx<'a, scroll::Endian>>::Error;
    fn try_from_ctx(src: &'a [u8], _ctx: scroll::Endian) -> Result<(Self, usize), Self::Error> {
        T::try_from_ctx(src, E::get_val()).map(|(val, size)| (EndianWrapper(val, PhantomData), size))
    }
}

impl<'a, T, E> TryIntoCtx<scroll::Endian> for &EndianWrapper<T, E>
    where T: TryIntoCtx<scroll::Endian> + Clone + 'a,
        E: Endian + 'a {
    type Error = <T as TryIntoCtx<scroll::Endian>>::Error;
    fn try_into_ctx(self, dest: &mut [u8], _ctx: scroll::Endian) -> Result<usize, Self::Error> {
        self.0.clone().try_into_ctx(dest, E::get_val())
    }
}

impl<'a, T, E> TryIntoCtx<scroll::Endian> for EndianWrapper<T, E>
    where T: TryIntoCtx<scroll::Endian> + Clone + 'a,
        E: Endian + 'a {
    type Error = <T as TryIntoCtx<scroll::Endian>>::Error;
    fn try_into_ctx(self, dest: &mut [u8], _ctx: scroll::Endian) -> Result<usize, Self::Error> {
        self.0.try_into_ctx(dest, E::get_val())
    }
}

impl<Ctx, T, E> MeasureWith<Ctx> for EndianWrapper<T, E> where T: MeasureWith<scroll::Endian>, E: Endian {
    #[inline]
    fn measure_with(&self, _ctx: &Ctx) -> usize {
        self.0.measure_with(&E::get_val())
    }
}

impl<T, E> IntoInner for EndianWrapper<T, E>
    where T: IntoInner {
    type Inner = <T as IntoInner>::Inner;
    fn into_inner(self) -> Self::Inner {
        self.0.into_inner()
    }
}

impl<T, E> FromInner for EndianWrapper<T, E> where T: FromInner {
    type Inner = <T as FromInner>::Inner;
    fn from_inner(inner: Self::Inner) -> Self {
        EndianWrapper(T::from_inner(inner), PhantomData)
    }
}

// for the sake of usage as a Length in LengthData
impl<T, Endian> TryFrom<EndianWrapper<T, Endian>> for usize
    where usize: TryFrom<T> {
    type Error = <usize as TryFrom<T>>::Error;
    fn try_from(src: EndianWrapper<T, Endian>) -> Result<Self, Self::Error> {
        src.0.try_into()
    }
}

impl<T, Endian> TryFrom<usize> for EndianWrapper<T, Endian>
    where T: TryFrom<usize> {
    type Error = <T as TryFrom<usize>>::Error;
    fn try_from(src: usize) -> Result<Self, Self::Error> {
        Ok(EndianWrapper(src.try_into()?, PhantomData))
    }
}

// Now that *that* is all out of the way... usage!

#[derive(Pread, Pwrite)]
struct Example {
    big: EndianWrapper<D<u16>, BigEndian>,
    little: EndianWrapper<D<u16>, LittleEndian>,
    var: LengthData<UTF16<LittleEndian>, EndianWrapper<u16, BigEndian>>
}

#[derive(Debug)]
struct ExampleUnwrapped {
    big: u16,
    little: u16,
    var: String
}

impl From<Example> for ExampleUnwrapped {
    fn from(src: Example) -> ExampleUnwrapped {
        ExampleUnwrapped {
            big: src.big.into_inner(),
            little: src.little.into_inner(),
            var: src.var.into_inner()
        }
    }
}

impl From<ExampleUnwrapped> for Example {
    fn from(src: ExampleUnwrapped) -> Example {
        Example {
            big: EndianWrapper::from_inner(src.big),
            little: EndianWrapper::from_inner(src.little),
            var: LengthData::from_inner(src.var)
        }
    }
}

fn main() {
    let src = [0u8, 42, 42, 0, 0, 10, 0x48, 0, 0x65, 0, 0x6c, 0, 0x6c, 0, 0x6f, 0];
    let mut dest = vec![0; src.len()];
    let example: Example = src.pread(0).unwrap();
    let unwrapped: ExampleUnwrapped = example.into();

    println!("{:?}", unwrapped);

    assert_eq!(unwrapped.big, 42u16);
    assert_eq!(unwrapped.little, 42u16);
    assert_eq!(unwrapped.var, "Hello".to_owned());

    let example: Example = unwrapped.into();
    dest.pwrite(example, 0).unwrap();

    assert_eq!(src.as_ref(), dest.as_slice());
}
