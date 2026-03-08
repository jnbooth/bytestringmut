# `bytestringmut`

Mutable manipulation of [`ByteString`](https://docs.rs/bytestring/latest/bytestring/struct.ByteString.html)s from the [`bytestring`](https://docs.rs/bytestring) crate.

[![crates.io](https://img.shields.io/crates/v/bytestringmut.svg)](https://crates.io/crates/bytestringmut)
[![Documentation](https://docs.rs/bytestringmut/badge.svg)](https://docs.rs/bytestringmut)
![MIT licensed](https://img.shields.io/crates/l/bytestringmut.svg)
<br />
[![Dependency Status](https://deps.rs/crate/bytestringmut/latest/status.svg)](https://deps.rs/crate/bytestringmut)
![Downloads](https://img.shields.io/crates/d/bytestringmut.svg)

## Usage

To use `bytestringmut`, first add this to your `Cargo.toml`:

```toml
[dependencies]
bytestringmut = "1"
```

Next, add this to your crate:

```rust
use bytestringmut::ByteStringMut;
```

## no_std support

To use `bytestringmut` with no_std environment, disable the (enabled by default) `std` feature.

```toml
[dependencies]
bytestringmut = { version = "1", default-features = false }
```

`bytestringmut` forwards the `std` feature to `bytes`. It also forwards the `extra-platforms` feature if enabled. See the [no_std documentation for the `bytes` crate](https://docs.rs/crate/bytes/latest) for more information.

## License

This project is licensed under the [MIT license](LICENSE).

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in `bytestringmut` by you, shall be licensed as MIT, without any
additional terms or conditions.
