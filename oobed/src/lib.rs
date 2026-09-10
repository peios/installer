pub mod flow;
pub mod flow_impl;
pub mod setup;

/// What oobed calls itself in operational messages.
///
/// It has more use for them than most daemons: it runs before anyone
/// can log in, so its stderr goes to a log nobody can read yet, and the
/// console it would otherwise complain on is the one its own surface is
/// drawing a form onto.
pub const TAG: &str = "oobed";
