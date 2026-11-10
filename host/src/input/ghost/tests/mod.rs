//! Tests of the pen routing, split by topic: `route` (app/route decisions), `pointer` (the
//! physical-tablet injection), `auto` (the Android auto style, click slop, hover parking) and
//! `state` (tracked device state and forced releases). Shared helpers are in `support`.

mod auto;
mod pointer;
mod route;
mod state;
mod support;
