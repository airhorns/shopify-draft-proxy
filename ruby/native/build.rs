use std::error::Error;

// Activates the rb-sys build environment so the cdylib links against the host
// Ruby correctly across platforms — notably, this injects the macOS
// `-undefined dynamic_lookup` linker flag so Ruby C-API symbols resolve at
// dlopen time (the flag a bare `cargo build` omits).
fn main() -> Result<(), Box<dyn Error>> {
    let _ = rb_sys_env::activate()?;
    Ok(())
}
