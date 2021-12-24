# rusty-router
A network router written in rust.

[![license](https://github.com/gazzasaur/status/blob/main/rusty-router/license.png?raw=true "License")](/LICENSE?raw=true)
![commit](https://github.com/gazzasaur/status/blob/main/rusty-router/commit.png?raw=true "Commit")
![build](https://github.com/gazzasaur/status/blob/main/rusty-router/build.png?raw=true "Build")
![coverage](https://github.com/gazzasaur/status/blob/main/rusty-router/coverage.png?raw=true "Coverage")

This is my first real-world application of Rust.  I mainly use Java and C++ today.  I am looking at Rust to replace my need for C++ as I have noticed I can develop faster in Rust.

I chose to implement a router as I have a passion for Routing Protocols.  Routing was my first real exposure to a large scale distributed systems.  I spent most of my time on monolithic systems before this.  I still see many of the concepts baked into RFCs from decades ago being applied as *new* techniques today.

## Design Decisions

##### Async vs Blocking
I have chosen to go async with this project.  There are many IO bound calls in dealing with protocol messaging and timers that
make more sense as an async process.

##### Trait vs Struct
Structs and generics have several performance advantages over traits.  However, traits allow for a level of abstraction that is not
easily achieved using structs and generics alone.

In this project, I plan to release a simulator platform.  To ensure I will have the required flexibility to do this, I have used
traits in several areas.  I tend to follow a set of guidelines to ensure I do not eat away at the performance of this project.
These are only guidelines and can be bent or broken, but not without a great deal of thought.

* Traits will be used at senible boundaries such that an expensive trait call will be followed by a large chunk of processing
* Traits will be used in control plane calls that are usually required to check a status or setup a longer running task

##### Object vs Ref vs Box vs Rc vs Arc vs etc
There are many ways objects can be mapped to ownership in rust.  Due to the liberal use of traits, many things will not be Sized.
Also, due to the use of Async tasks, there will be a need to pass several objects between threads.  In short, expect to see a lot
of 'dyn Trait + Send + Sync'.  These must be wrapped in something heap allocated.

For thw vast majority of these I have chosen to use an Arc rather than a Box (or other structure).  Arc is more expensive than the
other strutures.  However, as several structures, like Rc are !Send and !Sync, this feel short sighted in an Async project.
Also, Arc is only really incurs a penalty during atomic operations (clone and drop).  So a couple of guide lines can help here.

* If data is not mut and is Sized, no need to wrap
* If data is short lived, like returning 'dyn Error + Send + Sync', Box is okay as it removes the performance overhead of dropping an Arc
* If data is very frequently generated and cannot be Sized, Box is acceptable as it removes the overhead of dropping an Arc
* For anything else, prefer Arc

In many cases, it is a case of 'I would rather have it (an Arc) and not need to, than need it (I used Box instead) and not have it.'

##### Change
Guidelines change and project, processes, technology and people evolve.  If any APIs change as a result, there will be a major version bump.

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
