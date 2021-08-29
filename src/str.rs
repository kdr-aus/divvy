//! Stack allocated string.

use std::{
    borrow::{Borrow, Cow},
    cmp, fmt,
    hash::{Hash, Hasher},
    iter,
    ops::Deref,
    sync::Arc,
};
use S::*;

const STRARR_LEN: u8 = 22;

/// A stack allocated string.
///
/// `Str` has three variants, static, which captures static string slices, stack, which is a
/// 22 byte array, and heap, a [`String`]. When on the heap, the [`String`] is wrapped in an
/// [`Arc`] to provide cheap [`Clone`]ing of the value, just cloning the pointer.
/// Small strings are kept in the stack until a heap alloction is required.
pub struct Str(S);

impl Str {
    /// Create a `Str` from something that is a string reference.
    pub fn new<T>(string: T) -> Self
    where
        T: AsRef<str>,
    {
        let tmp = string.as_ref();
        let len = tmp.len();
        const LEN: usize = STRARR_LEN as usize;
        if len <= LEN {
            let mut arr = [0; LEN];
            let (left, _) = arr.split_at_mut(len);
            left.copy_from_slice(tmp.as_bytes());
            Self(Stack((len as u8, arr)))
        } else {
            Self(Heap(Arc::new(tmp.to_owned())))
        }
    }

    /// Represent the `Str` as a string slice.
    pub fn as_str(&self) -> &str {
        match &self.0 {
            Static(x) => x,
            Stack(x) => stack_as_str(x),
            Heap(x) => x.as_str(),
        }
    }

    /// Consume the stack string and turn into a string.
    ///
    /// Will allocate if necessary.
    pub fn into_string(self) -> String {
        match self.0 {
            Static(x) => x.to_owned(),
            Stack(x) => stack_as_str(&x).to_owned(),
            Heap(x) => x.as_ref().clone(),
        }
    }

    /// Acquires a mutable reference to the string.
    ///
    /// If not already heap allocated, clones and allocates the string on the heap.
    pub fn to_mut(&mut self) -> &mut String {
        let old = std::mem::replace(&mut self.0, Static(""));
        self.0 = match old {
            Static(x) => Heap(Arc::new(x.to_owned())),
            Stack(x) => Heap(Arc::new(stack_as_str(&x).to_owned())),
            Heap(x) => Heap(x),
        };
        match &mut self.0 {
            Heap(x) => Arc::make_mut(x),
            _ => unreachable!("ensured all arms were transformed into Heap"),
        }
    }

    /// Does a pointer equality check of two [`Str`]s.
    ///
    /// Two [`Str`]s can share the same pointer to either a shared `&'static str`, or a shared
    /// `Arc<String>`.
    ///
    /// # Example
    /// ```rust
    /// # use std::sync::Arc;
    /// # use divvy::Str;
    /// let a = Arc::new("Hello".to_string());
    /// let b = Arc::new("Hello".to_string());
    /// let a1 = Str::from(&a);
    /// let a2 = Str::from(&a);
    /// let b1 = Str::from(&b);
    ///
    /// assert_eq!(Str::ptr_eq(&a1, &a2), true);
    /// assert_eq!(Str::ptr_eq(&a1, &b1), false);
    /// assert_eq!(a1.eq(&b1), true); // value equality
    /// ```
    pub fn ptr_eq(this: &Str, other: &Str) -> bool {
        match (&this.0, &other.0) {
            (Static(a), Static(b)) => std::ptr::eq(*a, *b),
            (Heap(a), Heap(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}

impl fmt::Debug for Str {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self.as_str())
    }
}

impl fmt::Display for Str {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.pad(self)
    }
}

impl AsRef<str> for Str {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Deref for Str {
    type Target = str;
    fn deref(&self) -> &str {
        self.as_ref()
    }
}

impl Borrow<str> for Str {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl Default for Str {
    fn default() -> Self {
        Self(Heap(Arc::new(String::new())))
    }
}

impl std::str::FromStr for Str {
    type Err = std::convert::Infallible;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Str::new(s))
    }
}

impl serde::Serialize for Str {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}
impl<'de> serde::Deserialize<'de> for Str {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        <&str>::deserialize(deserializer).map(Str::new)
    }
}

// ########### FROM CONVERSIONS ###############################################
impl From<&'static str> for Str {
    fn from(x: &'static str) -> Self {
        Self(Static(x))
    }
}

impl From<String> for Str {
    fn from(x: String) -> Self {
        Self(Heap(Arc::new(x)))
    }
}

impl From<&String> for Str {
    fn from(x: &String) -> Self {
        Str::new(x)
    }
}

impl From<&mut String> for Str {
    fn from(x: &mut String) -> Self {
        Str::new(x)
    }
}

impl From<Arc<String>> for Str {
    fn from(x: Arc<String>) -> Self {
        Str(Heap(x))
    }
}

impl From<&Arc<String>> for Str {
    fn from(x: &Arc<String>) -> Self {
        Str(Heap(Arc::clone(x)))
    }
}

impl From<Cow<'static, str>> for Str {
    fn from(x: Cow<'static, str>) -> Self {
        match x {
            Cow::Borrowed(x) => Str::from(x),
            Cow::Owned(x) => Str::from(x),
        }
    }
}

impl iter::FromIterator<char> for Str {
    fn from_iter<I: IntoIterator<Item = char>>(iter: I) -> Self {
        Self::from(iter.into_iter().collect::<String>())
    }
}

// ########### COMPARISONS ####################################################
impl PartialEq for Str {
    fn eq(&self, other: &Self) -> bool {
        if Str::ptr_eq(self, other) {
            true
        } else {
            self.as_str() == other.as_str()
        }
    }
}
impl Eq for Str {}

impl PartialEq<str> for Str {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl Ord for Str {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        if Str::ptr_eq(self, other) {
            cmp::Ordering::Equal
        } else {
            self.as_str().cmp(other.as_str())
        }
    }
}

impl PartialOrd for Str {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialOrd<str> for Str {
    fn partial_cmp(&self, other: &str) -> Option<cmp::Ordering> {
        Some(self.as_str().cmp(other))
    }
}

impl Hash for Str {
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        self.as_str().hash(hasher)
    }
}

impl Clone for Str {
    fn clone(&self) -> Self {
        match &self.0 {
            Static(s) => Self(Static(s)),
            Stack((len, arr)) => {
                let len = *len as usize;
                let mut narr = [0; STRARR_LEN as usize];
                let (left, _) = narr.split_at_mut(len);
                left.copy_from_slice(&arr[..len]);
                Self(Stack((len as u8, narr)))
            }
            Heap(s) => Self(Heap(Arc::clone(s))),
        }
    }
}

type StrArr = (u8, [u8; STRARR_LEN as usize]);

enum S {
    Static(&'static str),
    Stack(StrArr),
    #[allow(clippy::rc_buffer)]
    Heap(Arc<String>),
}

fn stack_as_str(s: &StrArr) -> &str {
    unsafe { std::str::from_utf8_unchecked(&(s.1)[..s.0 as usize]) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_into_string() {
        let expecting = String::from("Hello, world!");
        let ss = Str(Static("Hello, world!"));
        assert_eq!(ss.into_string(), expecting);
        let ss = Str::new("Hello, world!");
        assert_eq!(ss.into_string(), expecting);
        let ss = Str(Heap(Arc::new(expecting.clone())));
        assert_eq!(ss.into_string(), expecting);
    }

    #[test]
    fn test_to_mut() {
        let expecting = "Hello, world!";

        let mut ss = Str(Static(""));
        ss.to_mut().push_str(expecting);
        assert_eq!(ss.as_str(), expecting);

        let mut ss = Str::new("");
        ss.to_mut().push_str(expecting);
        assert_eq!(ss.as_str(), expecting);

        let mut ss = Str(Heap(Arc::new("".into())));
        ss.to_mut().push_str(expecting);
        assert_eq!(ss.as_str(), expecting);
    }

    #[test]
    fn debug_test() {
        let expecting = format!("{:?}", String::from("Hello, world!"));
        let ss = Str(Static("Hello, world!"));
        assert_eq!(format!("{:?}", ss), expecting);
        let ss = Str::from("Hello, world!");
        assert_eq!(format!("{:?}", ss), expecting);
        let ss = Str(Heap(Arc::new("Hello, world!".into())));
        assert_eq!(format!("{:?}", ss), expecting);
    }

    #[test]
    fn fmting_test() {
        let s = format!("{:15} here", Str::from("Hello"));
        assert_eq!(&s, "Hello           here");
    }

    #[test]
    fn ord_test() {
        let s1 = Str::from("a");
        let s2 = Str::from("b");
        assert_eq!(s1.partial_cmp(&s2), Some(cmp::Ordering::Less));
    }

    #[test]
    fn ptr_eq_testing() {
        let a = "Hello";
        let b = "Hello";
        let c = Str(Static(a));
        let a = Str(Static(a));
        let b = Str(Static(b));

        assert_eq!(a.eq(&b), true);
        assert_eq!(Str::ptr_eq(&a, &a), true);
        assert_eq!(Str::ptr_eq(&a, &b), true); // Rust interns the same static strs
        assert_eq!(Str::ptr_eq(&a, &c), true);

        let a = "Hello";
        let b = "Hello";
        let a = Str(Heap(Arc::new(a.to_string())));
        let b = Str(Heap(Arc::new(b.to_string())));
        let c = a.clone();

        assert_eq!(a.eq(&b), true);
        assert_eq!(Str::ptr_eq(&a, &a), true);
        assert_eq!(Str::ptr_eq(&a, &b), false);
        assert_eq!(Str::ptr_eq(&a, &c), true);

        assert_eq!(Str::ptr_eq(&a, &Str::from("Hello")), false);

        let a = Arc::new("Hello".to_string());
        let b = Arc::new("Hello".to_string());

        let a1 = Str::from(&a);
        let a2 = Str::from(&a);
        let b1 = Str::from(&b);

        assert_eq!(a1.eq(&b1), true);
        assert_eq!(Str::ptr_eq(&a1, &a2), true);
        assert_eq!(a1.eq(&a2), true);
        assert_eq!(Str::ptr_eq(&a1, &b1), false);
        assert_eq!(a1.cmp(&a2), cmp::Ordering::Equal);
    }

    #[test]
    fn from_testing() {
        let s = Str::from("Hello, world");
        match s.0 {
            Static(_) => (),
            _ => panic!("expecting this variant"),
        }

        let s = Str::from("Hello, world".to_string());
        match s.0 {
            Heap(_) => (),
            _ => panic!("expecting this variant"),
        }

        let s = Str::from(&"Hello, world".to_string());
        match s.0 {
            Stack(_) => (),
            _ => panic!("expecting this variant"),
        }

        let s = Str::from(&mut "Hello, world".to_string());
        match s.0 {
            Stack(_) => (),
            _ => panic!("expecting this variant"),
        }

        let s = Str::from(Arc::new("Hello, world".to_string()));
        match s.0 {
            Heap(_) => (),
            _ => panic!("expecting this variant"),
        }

        let s = Str::from(Cow::Borrowed("Hello, world"));
        match s.0 {
            Static(_) => (),
            _ => panic!("expecting this variant"),
        }

        let s = Str::from(Cow::Owned("Hello, world".to_string()));
        match s.0 {
            Heap(_) => (),
            _ => panic!("expecting this variant"),
        }
    }
}
