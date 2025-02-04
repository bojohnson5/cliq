use rust_daq::*;

fn main() -> Result<(), FELibError> {
    println!("Hello, world!");
    let res = felib_open("dig2://caendgtz-usb-25380");
    match res {
        Ok(v) => println!("opened digitizer with handle: {}", v),
        Err(e) => println!("can't open digitizer: {:?}", e),
    }

    Ok(())
}
