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
/// `DStr` has three variants, static, which captures static string slices, stack, which is a
/// 22 byte array, and heap, a [`String`]. When on the heap, the [`String`] is wrapped in an
/// [`Arc`] to provide cheap [`Clone`]ing of the value, just cloning the pointer.
/// Small strings are kept in the stack until a heap alloction is required.
pub struct DStr(S);

impl DStr {
    /// Create a `DStr` from something that is a string reference.
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

    /// Represent the `DStr` as a string slice.
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

    /// Does a pointer equality check of two [`DStr`]s.
    ///
    /// Two [`DStr`]s can share the same pointer to either a shared `&'static str`, or a shared
    /// `Arc<String>`.
    ///
    /// # Example
    /// ```rust
    /// # use std::sync::Arc;
    /// # use divvy::DStr;
    /// let a = Arc::new("Hello".to_string());
    /// let b = Arc::new("Hello".to_string());
    /// let a1 = DStr::from(&a);
    /// let a2 = DStr::from(&a);
    /// let b1 = DStr::from(&b);
    ///
    /// assert_eq!(DStr::ptr_eq(&a1, &a2), true);
    /// assert_eq!(DStr::ptr_eq(&a1, &b1), false);
    /// assert_eq!(a1.eq(&b1), true); // value equality
    /// ```
    pub fn ptr_eq(this: &DStr, other: &DStr) -> bool {
        match (&this.0, &other.0) {
            (Static(a), Static(b)) => std::ptr::eq(*a, *b),
            (Heap(a), Heap(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}

impl fmt::Debug for DStr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self.as_str())
    }
}

impl fmt::Display for DStr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.pad(self)
    }
}

impl AsRef<str> for DStr {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Deref for DStr {
    type Target = str;
    fn deref(&self) -> &str {
        self.as_ref()
    }
}

impl Borrow<str> for DStr {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl Default for DStr {
    fn default() -> Self {
        Self(Heap(Arc::new(String::new())))
    }
}

impl std::str::FromStr for DStr {
    type Err = std::convert::Infallible;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(DStr::new(s))
    }
}

impl serde::Serialize for DStr {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}
impl<'de> serde::Deserialize<'de> for DStr {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        <&str>::deserialize(deserializer).map(DStr::new)
    }
}

// ########### FROM CONVERSIONS ###############################################
impl From<&'static str> for DStr {
    fn from(x: &'static str) -> Self {
        Self(Static(x))
    }
}

impl From<String> for DStr {
    fn from(x: String) -> Self {
        Self(Heap(Arc::new(x)))
    }
}

impl From<&String> for DStr {
    fn from(x: &String) -> Self {
        DStr::new(x)
    }
}

impl From<&mut String> for DStr {
    fn from(x: &mut String) -> Self {
        DStr::new(x)
    }
}

impl From<Arc<String>> for DStr {
    fn from(x: Arc<String>) -> Self {
        DStr(Heap(x))
    }
}

impl From<&Arc<String>> for DStr {
    fn from(x: &Arc<String>) -> Self {
        DStr(Heap(Arc::clone(x)))
    }
}

impl From<Cow<'static, str>> for DStr {
    fn from(x: Cow<'static, str>) -> Self {
        match x {
            Cow::Borrowed(x) => DStr::from(x),
            Cow::Owned(x) => DStr::from(x),
        }
    }
}

impl iter::FromIterator<char> for DStr {
    fn from_iter<I: IntoIterator<Item = char>>(iter: I) -> Self {
        Self::from(iter.into_iter().collect::<String>())
    }
}

// ########### COMPARISONS ####################################################
impl PartialEq for DStr {
    fn eq(&self, other: &Self) -> bool {
        if DStr::ptr_eq(self, other) {
            true
        } else {
            self.as_str() == other.as_str()
        }
    }
}
impl Eq for DStr {}

impl PartialEq<str> for DStr {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl Ord for DStr {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        if DStr::ptr_eq(self, other) {
            cmp::Ordering::Equal
        } else {
            self.as_str().cmp(other.as_str())
        }
    }
}

impl PartialOrd for DStr {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialOrd<str> for DStr {
    fn partial_cmp(&self, other: &str) -> Option<cmp::Ordering> {
        Some(self.as_str().cmp(other))
    }
}

impl Hash for DStr {
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        self.as_str().hash(hasher)
    }
}

impl Clone for DStr {
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
        let ss = DStr(Static("Hello, world!"));
        assert_eq!(ss.into_string(), expecting);
        let ss = DStr::new("Hello, world!");
        assert_eq!(ss.into_string(), expecting);
        let ss = DStr(Heap(Arc::new(expecting.clone())));
        assert_eq!(ss.into_string(), expecting);
    }

    #[test]
    fn test_to_mut() {
        let expecting = "Hello, world!";

        let mut ss = DStr(Static(""));
        ss.to_mut().push_str(expecting);
        assert_eq!(ss.as_str(), expecting);

        let mut ss = DStr::new("");
        ss.to_mut().push_str(expecting);
        assert_eq!(ss.as_str(), expecting);

        let mut ss = DStr(Heap(Arc::new("".into())));
        ss.to_mut().push_str(expecting);
        assert_eq!(ss.as_str(), expecting);
    }

    #[test]
    fn debug_test() {
        let expecting = format!("{:?}", String::from("Hello, world!"));
        let ss = DStr(Static("Hello, world!"));
        assert_eq!(format!("{:?}", ss), expecting);
        let ss = DStr::from("Hello, world!");
        assert_eq!(format!("{:?}", ss), expecting);
        let ss = DStr(Heap(Arc::new("Hello, world!".into())));
        assert_eq!(format!("{:?}", ss), expecting);
    }

    #[test]
    fn fmting_test() {
        let s = format!("{:15} here", DStr::from("Hello"));
        assert_eq!(&s, "Hello           here");
    }

    #[test]
    fn ord_test() {
        let s1 = DStr::from("a");
        let s2 = DStr::from("b");
        assert_eq!(s1.partial_cmp(&s2), Some(cmp::Ordering::Less));
    }

    #[test]
    fn ptr_eq_testing() {
        let a = "Hello";
        let b = "Hello";
        let c = DStr(Static(a));
        let a = DStr(Static(a));
        let b = DStr(Static(b));

        assert_eq!(a.eq(&b), true);
        assert_eq!(DStr::ptr_eq(&a, &a), true);
        assert_eq!(DStr::ptr_eq(&a, &b), true); // Rust interns the same static strs
        assert_eq!(DStr::ptr_eq(&a, &c), true);

        let a = "Hello";
        let b = "Hello";
        let a = DStr(Heap(Arc::new(a.to_string())));
        let b = DStr(Heap(Arc::new(b.to_string())));
        let c = a.clone();

        assert_eq!(a.eq(&b), true);
        assert_eq!(DStr::ptr_eq(&a, &a), true);
        assert_eq!(DStr::ptr_eq(&a, &b), false);
        assert_eq!(DStr::ptr_eq(&a, &c), true);

        assert_eq!(DStr::ptr_eq(&a, &DStr::from("Hello")), false);

        let a = Arc::new("Hello".to_string());
        let b = Arc::new("Hello".to_string());

        let a1 = DStr::from(&a);
        let a2 = DStr::from(&a);
        let b1 = DStr::from(&b);

        assert_eq!(a1.eq(&b1), true);
        assert_eq!(DStr::ptr_eq(&a1, &a2), true);
        assert_eq!(a1.eq(&a2), true);
        assert_eq!(DStr::ptr_eq(&a1, &b1), false);
        assert_eq!(a1.cmp(&a2), cmp::Ordering::Equal);
    }

    #[test]
    fn from_testing() {
        let s = DStr::from("Hello, world");
        match s.0 {
            Static(_) => (),
            _ => panic!("expecting this variant"),
        }

        let s = DStr::from("Hello, world".to_string());
        match s.0 {
            Heap(_) => (),
            _ => panic!("expecting this variant"),
        }

        let s = DStr::from(&"Hello, world".to_string());
        match s.0 {
            Stack(_) => (),
            _ => panic!("expecting this variant"),
        }

        let s = DStr::from(&mut "Hello, world".to_string());
        match s.0 {
            Stack(_) => (),
            _ => panic!("expecting this variant"),
        }

        let s = DStr::from(Arc::new("Hello, world".to_string()));
        match s.0 {
            Heap(_) => (),
            _ => panic!("expecting this variant"),
        }

        let s = DStr::from(Cow::Borrowed("Hello, world"));
        match s.0 {
            Static(_) => (),
            _ => panic!("expecting this variant"),
        }

        let s = DStr::from(Cow::Owned("Hello, world".to_string()));
        match s.0 {
            Heap(_) => (),
            _ => panic!("expecting this variant"),
        }
    }
}
