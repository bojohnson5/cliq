use rust_daq::felib_open;

fn main() {
    println!("Hello, world!");
    let res = felib_open("dig2://caendgtz-usb-25380");
    match res {
        Ok(v) => println!("opened digitizer with handle: {}", v),
        Err(e) => println!("can't open digitizer: {:?}", e),
    }
}
