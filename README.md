# rusty-router
A network router written in rust.

This is my first real-world application of Rust.  I mainly use Java and C++ today.  I am looking at Rust to replace my need for C++ as I have noticed I can develop faster in Rust.

I chose to implement a router as I have a passion for Routing Protocols.  Routing was my first real exposure to a large scale distributed systems.  I spent most of my time on monolithic systems before this.  I still see many of the concepts baked into RFCs from decades ago being applied as *new* techniques today.

## Development

##### Run Tests
```
RUST_BACKTRACE=1; cargo test
```

##### Run Tests with Coverage
At present Tarpaulin only supports x86_64 builds.

* Install Tarpaulin
```
cargo install cargo-tarpaulin
```
* Run tests with coverage
```
RUST_BACKTRACE=1; cargo tarpaulin --ignore-tests
```

## Road-map
* IPv4
  * Virtal Route Table (Static Routes)
  * Forward Table Interface
  * OSPFv2
  * OSPFv2 Physical Interface
  * OSPFv2 Virtual Interface
  * OSPFv2 Testing Platform (most likely with Quagga interop testing)
  * Management Console
  * BGPv4
  * LDP (MPLS)
  * RSVP-TE (MPLS)
* IPv6
  * OSPFv3
  * BGPv4
  * LDP (MPLS)
  * RSVP-TE (MPLS)
* Other
  * IS-IS
