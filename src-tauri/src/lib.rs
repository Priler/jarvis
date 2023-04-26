// Taken from https://github.com/Vurich/const-concat/issues/13

#![no_std]

use core::mem::ManuallyDrop;

const unsafe fn transmute_prefix<From, To>(from: From) -> To {
    union Transmute<From, To> {
        from: ManuallyDrop<From>,
        to: ManuallyDrop<To>,
    }

    ManuallyDrop::into_inner(
        Transmute {
            from: ManuallyDrop::new(from),
        }
        .to,
    )
}

/// # Safety
///
/// `Len1 + Len2 >= Len3`
#[doc(hidden)]
#[allow(non_upper_case_globals)]
pub const unsafe fn concat<const Len1: usize, const Len2: usize, const Len3: usize>(
    arr1: [u8; Len1],
    arr2: [u8; Len2],
) -> [u8; Len3] {
    #[repr(C)]
    struct Concat<A, B>(A, B);
    transmute_prefix(Concat(arr1, arr2))
}

#[macro_export]
macro_rules! const_concat {
    () => ("");
    ($a:expr) => ($a);

    ($a:expr, $b:expr $(,)?) => {{
        const A: &str = $a;
        const B: &str = $b;
        const BYTES: [u8; { A.len() + B.len() }] = unsafe {
            $crate::concat::<
                { A.len() },
                { B.len() },
                { A.len() + B.len() }
            >(
                *A.as_ptr().cast(),
                *B.as_ptr().cast(),
            )
        };
        unsafe { ::core::str::from_utf8_unchecked(&BYTES) }
    }};

    ($a:expr, $b:expr, $($rest:expr),+ $(,)?) => {{
        const TAIL: &str = $crate::const_concat!($b, $($rest),+);
        $crate::const_concat!($a, TAIL)
    }}
}

#[test]
fn tests() {
    const SALUTATION: &str = "Hello";
    const TARGET: &str = "world";
    const GREETING: &str = const_concat!(SALUTATION, ", ", TARGET, "!");
    const GREETING_TRAILING_COMMA: &str = const_concat!(SALUTATION, ", ", TARGET, "!",);

    assert_eq!(GREETING, "Hello, world!");
    assert_eq!(GREETING_TRAILING_COMMA, "Hello, world!");
}
