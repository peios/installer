//! oobe-gxwi — first-boot setup in a browser, GXWI's overlay on an installed
//! machine until it has been set up. oobed makes it the overlay, and takes it
//! away again. See the library for how it works.

fn main() {
    installer_gxwi::run(installer_gxwi::Conversation::Oobe);
}
