use num_complex::*;
use std::f64::consts::PI;

pub struct FFTransformer {
    buffer : Vec<Complex<f64>>,
}

pub enum Direction {
    FORWARD,
    BACKWARD,
}

impl FFTransformer {

    pub fn new() -> Self {
        FFTransformer { buffer: Vec::new() }
    }

    fn basic_fft(
        &mut self,
        xs: &Vec<Complex<f64>>,
        transformed_xs: &mut Vec<Complex<f64>>,
        exp: Complex<f64>,
        start: usize,
        step: usize,
        n: usize,
    ) {
        debug_assert!(xs.len() == transformed_xs.len() && xs.len().is_power_of_two());
        let index = |i: usize| start + step * i;
    
        if n == 2 {
            transformed_xs[index(0)] = xs[index(0)] + xs[index(1)];
            transformed_xs[index(1)] = xs[index(0)] - xs[index(1)];
            return;
        }
    
        let next_exp = exp * exp;
        self.basic_fft(xs, transformed_xs, next_exp, start, step * 2, n / 2);
        self.basic_fft(xs, transformed_xs, next_exp, start + step, step * 2, n / 2);
    
        let mut current_exp = Complex::<f64>::new(1.0, 0.0);
        for i in 0..n / 2 {
            let (even_term, mut odd_term) = if i >= n / 4 {
                (self.buffer[i % (n / 4)], self.buffer[i % (n / 4) + 1])
            } else {
                (transformed_xs[index(2 * i)], transformed_xs[index(2 * i + 1)])
            };
            odd_term *= current_exp;

            self.buffer[i % (n / 4)] = transformed_xs[index(i + n / 2)];

            transformed_xs[index(i)] = even_term + odd_term;
            transformed_xs[index(i + n / 2)] = even_term - odd_term;
            current_exp = current_exp * exp;
        }
    }
    
    pub fn fft(&mut self, xs: &Vec<Complex<f64>>, dir: Direction) -> Vec<Complex<f64>> {
        let len = xs.len();

        if self.buffer.len() < len / 4 {
            self.buffer.resize(len / 4 + 1, Complex::new(0.0, 0.0));
        }

        let angle =  2.0 * PI * (len as f64) * match dir {
            Direction::FORWARD   => -1.0,
            Direction::BACKWARD  =>  1.0,
        };
        let mut transformed_xs = vec![Complex::<f64>::new(0.0, 0.0); len];
        let exp = Complex::<f64>::from_polar(1.0, angle);
        self.basic_fft(xs, &mut transformed_xs, exp, 0, 1, len);
        transformed_xs
    }
    
    pub fn normalise(xs: &mut Vec<Complex<f64>>) {
        let len = xs.len();
        for i in 0..len {
            xs[i] /= len as f64;
        }
    }
}

