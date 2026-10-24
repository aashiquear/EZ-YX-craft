//! # ezyx-gcode
//!
//! * [`post`] – data-driven post-processors. A post is a small JSON document (see
//!   `PostConfig`), so supporting a new machine usually means adding a file, not code.
//! * [`parse`] – a tolerant G-code reader (G0/G1/G2/G3, G20/G21, G90/G91, A axis) used for
//!   previewing external files, simulation and streaming progress.

pub mod parse;
pub mod post;

pub use parse::{parse, ParsedMove};
pub use post::{builtin_posts, emit, PostConfig};
