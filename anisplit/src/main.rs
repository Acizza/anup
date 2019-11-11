use clap::clap_app;

fn main() {
    let _ = clap_app!(anisplit =>
        (version: env!("CARGO_PKG_VERSION"))
        (author: env!("CARGO_PKG_AUTHORS"))
        (about: "This is a tool to split up an anime series that has multiple \
                 seasons merged together.")
        (@arg path: +takes_value +required "The path pointing to the series to split")
    )
    .get_matches();
}
