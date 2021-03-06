//! Elliptic Curve Digital Signature Algorithm (ECDSA) as specified in
//! [FIPS 186-4][1] (Digital Signature Standard)
//!
//! ## About
//!
//! This crate provides generic ECDSA support which can be used in the
//! following ways:
//!
//! - Generic implementation of ECDSA usable with the following crates:
//!   - [`k256`] (secp256k1)
//!   - [`p256`] (NIST P-256)
//! - Other crates which provide their own complete implementations of ECDSA can
//!   also leverage the types from this crate to export ECDSA functionality in a
//!   generic, interoperable way by leveraging the [`Signature`] type with the
//!   [`signature::Signer`] and [`signature::Verifier`] traits.
//!
//! [1]: https://csrc.nist.gov/publications/detail/fips/186/4/final
//! [`k256`]: https://docs.rs/k256
//! [`p256`]: https://docs.rs/p256

#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![forbid(unsafe_code)]
#![warn(missing_docs, rust_2018_idioms, intra_doc_link_resolution_failure)]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/RustCrypto/meta/master/logo_small.png",
    html_root_url = "https://docs.rs/ecdsa/0.7.2"
)]

pub mod asn1;

#[cfg(feature = "dev")]
#[cfg_attr(docsrs, doc(cfg(feature = "dev")))]
pub mod dev;

#[cfg(feature = "hazmat")]
#[cfg_attr(docsrs, doc(cfg(feature = "hazmat")))]
pub mod hazmat;

#[cfg(feature = "signer")]
#[cfg_attr(docsrs, doc(cfg(feature = "signer")))]
pub mod signer;

#[cfg(feature = "verifier")]
#[cfg_attr(docsrs, doc(cfg(feature = "verifier")))]
pub mod verifier;

// Re-export the `elliptic-curve` crate (and select types)
pub use elliptic_curve::{
    self, generic_array,
    weierstrass::{Curve, PublicKey},
    SecretKey,
};

// Re-export the `signature` crate (and select types)
pub use signature::{self, Error};

#[cfg(feature = "signer")]
pub use signer::Signer;

#[cfg(feature = "verifier")]
pub use verifier::Verifier;

use core::{
    convert::TryFrom,
    fmt::{self, Debug},
    ops::Add,
};
use elliptic_curve::{Arithmetic, ElementBytes, FromBytes};
use generic_array::{typenum::Unsigned, ArrayLength, GenericArray};

/// Size of a fixed sized signature for the given elliptic curve.
pub type SignatureSize<C> = <<C as elliptic_curve::Curve>::ElementSize as Add>::Output;

/// Fixed-size byte array containing an ECDSA signature
pub type SignatureBytes<C> = GenericArray<u8, SignatureSize<C>>;

/// ECDSA signatures (fixed-size).
///
/// Generic over elliptic curve types.
///
/// These signatures are serialized as fixed-sized big endian scalar values
/// with no additional framing:
///
/// - `r`: field element size for the given curve, big-endian
/// - `s`: field element size for the given curve, big-endian
///
/// For example, in a curve with a 256-bit modulus like NIST P-256 or
/// secp256k1, `r` and `s` will both be 32-bytes, resulting in a signature
/// with a total of 64-bytes.
///
/// ASN.1 is also supported via the [`Signature::from_asn1`] and
/// [`Signature::to_asn1`] methods.
#[derive(Clone, Eq, PartialEq)]
pub struct Signature<C: Curve>
where
    SignatureSize<C>: ArrayLength<u8>,
{
    bytes: SignatureBytes<C>,
}

impl<C: Curve> Signature<C>
where
    SignatureSize<C>: ArrayLength<u8>,
{
    /// Create a [`Signature`] from the serialized `r` and `s` components
    pub fn from_scalars(r: &ElementBytes<C>, s: &ElementBytes<C>) -> Self {
        let mut bytes = SignatureBytes::<C>::default();
        let scalar_size = C::ElementSize::to_usize();
        bytes[..scalar_size].copy_from_slice(r.as_slice());
        bytes[scalar_size..].copy_from_slice(s.as_slice());
        Signature { bytes }
    }

    /// Parse a signature from ASN.1 DER
    pub fn from_asn1(bytes: &[u8]) -> Result<Self, Error>
    where
        C::ElementSize: Add + ArrayLength<u8>,
        asn1::MaxSize<C>: ArrayLength<u8>,
        <C::ElementSize as Add>::Output: Add<asn1::MaxOverhead> + ArrayLength<u8>,
    {
        asn1::Signature::<C>::try_from(bytes).map(Into::into)
    }

    /// Serialize this signature as ASN.1 DER
    pub fn to_asn1(&self) -> asn1::Signature<C>
    where
        C::ElementSize: Add + ArrayLength<u8>,
        asn1::MaxSize<C>: ArrayLength<u8>,
        <C::ElementSize as Add>::Output: Add<asn1::MaxOverhead> + ArrayLength<u8>,
    {
        asn1::Signature::from_scalars(self.r(), self.s())
    }

    /// Get the `r` component of this signature
    pub fn r(&self) -> &ElementBytes<C> {
        ElementBytes::<C>::from_slice(&self.bytes[..C::ElementSize::to_usize()])
    }

    /// Get the `s` component of this signature
    pub fn s(&self) -> &ElementBytes<C> {
        ElementBytes::<C>::from_slice(&self.bytes[C::ElementSize::to_usize()..])
    }
}

impl<C> Signature<C>
where
    C: Curve + Arithmetic,
    C::Scalar: NormalizeLow,
    SignatureSize<C>: ArrayLength<u8>,
{
    /// Normalize signature into "low S" form as described in
    /// [BIP 0062: Dealing with Malleability][1].
    ///
    /// [1]: https://github.com/bitcoin/bips/blob/master/bip-0062.mediawiki
    pub fn normalize_s(&mut self) -> Result<bool, Error> {
        let s_bytes = GenericArray::from_mut_slice(&mut self.bytes[C::ElementSize::to_usize()..]);
        let s_option = C::Scalar::from_bytes(s_bytes);

        // Not constant time, but we're operating on public values
        if s_option.is_some().into() {
            let (s_low, was_high) = s_option.unwrap().normalize_low();

            if was_high {
                s_bytes.copy_from_slice(&s_low.into());
                Ok(true)
            } else {
                Ok(false)
            }
        } else {
            Err(Error::new())
        }
    }
}

impl<C: Curve> signature::Signature for Signature<C>
where
    SignatureSize<C>: ArrayLength<u8>,
{
    fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        Self::try_from(bytes)
    }
}

impl<C: Curve> AsRef<[u8]> for Signature<C>
where
    SignatureSize<C>: ArrayLength<u8>,
{
    fn as_ref(&self) -> &[u8] {
        self.bytes.as_slice()
    }
}

impl<C: Curve> Copy for Signature<C>
where
    SignatureSize<C>: ArrayLength<u8>,
    <SignatureSize<C> as ArrayLength<u8>>::ArrayType: Copy,
{
}

impl<C: Curve> Debug for Signature<C>
where
    SignatureSize<C>: ArrayLength<u8>,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ecdsa::Signature<{:?}>({:?})",
            C::default(),
            self.as_ref()
        )
    }
}

impl<C: Curve> TryFrom<&[u8]> for Signature<C>
where
    SignatureSize<C>: ArrayLength<u8>,
{
    type Error = Error;

    fn try_from(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() == <SignatureSize<C>>::to_usize() {
            Ok(Self {
                bytes: GenericArray::clone_from_slice(bytes),
            })
        } else {
            Err(Error::new())
        }
    }
}

impl<C> From<asn1::Signature<C>> for Signature<C>
where
    C: Curve,
    C::ElementSize: Add + ArrayLength<u8>,
    asn1::MaxSize<C>: ArrayLength<u8>,
    <C::ElementSize as Add>::Output: Add<asn1::MaxOverhead> + ArrayLength<u8>,
{
    fn from(doc: asn1::Signature<C>) -> Signature<C> {
        let mut bytes = SignatureBytes::<C>::default();
        let scalar_size = C::ElementSize::to_usize();
        let r_begin = scalar_size.checked_sub(doc.r().len()).unwrap();
        let s_begin = bytes.len().checked_sub(doc.s().len()).unwrap();

        bytes[r_begin..scalar_size].copy_from_slice(doc.r());
        bytes[s_begin..].copy_from_slice(doc.s());
        Signature { bytes }
    }
}

/// Normalize a scalar (i.e. ECDSA S) to the lower half the field, as described
/// in [BIP 0062: Dealing with Malleability][1].
///
/// [1]: https://github.com/bitcoin/bips/blob/master/bip-0062.mediawiki
pub trait NormalizeLow: Sized {
    /// Normalize scalar to the lower half of the field (i.e. negate it if it's
    /// larger than half the curve's order).
    /// Returns a tuple with the new scalar and a boolean indicating whether the given scalar
    /// was in the higher half.
    ///
    /// May be implemented to work in variable time.
    fn normalize_low(&self) -> (Self, bool);
}
