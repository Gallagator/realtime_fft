mod fft;

use num_complex::*;

fn main() {
    let v = vec![
        Complex::new(1.0, 0.0),
        Complex::new(0.0, 0.0),
        Complex::new(0.0, 0.0),
        Complex::new(0.0, 0.0),
        Complex::new(0.0, 0.0),
        Complex::new(0.0, 0.0),
        Complex::new(0.0, 0.0),
        Complex::new(0.0, 0.0),
 
    ];
    let mut fft = fft::FFTransformer::new();
    let transformed_x = fft.fft(&v, fft::Direction::FORWARD);
    println!("{:#?}", transformed_x);
}
