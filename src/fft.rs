//! Module to perform FFTs with.

use num_complex::*;
use std::f64::consts::PI;

/// A struct which can perform FFTs on buffers.
pub struct FFTransformer {
    buffer : Vec<Complex<f64>>,
}

/// Enum to specify which direction an FFT should take.
pub enum Direction {
    FORWARD,
    BACKWARD,
}

impl FFTransformer {

    /// Creates a new FFTransformer.
    pub fn new() -> Self {
        FFTransformer { buffer: Vec::new() }
    }

    /// Recursive algorithm to calculate fft.
    fn basic_fft(
        &mut self,
        xs: &Vec<Complex<f64>>,
        transformed_xs: &mut Vec<Complex<f64>>,
        exp: Complex<f64>,
        start: usize,
        step: usize,
        n: usize,
    ) {
        // Ensure buffers are of equal size and that the size is a power of 2.
        debug_assert!(xs.len() == transformed_xs.len() && xs.len().is_power_of_two());
        // Closure to index even and odd sub-arrays properly.
        let index = |i: usize| start + step * i;
    
        // Base case: FFT for buffer of size 2
        if n == 2 {
            transformed_xs[index(0)] = xs[index(0)] + xs[index(1)];
            transformed_xs[index(1)] = xs[index(0)] - xs[index(1)];
            return;
        }
    
        // Angle of complex sinusoid doubles in the recursive case.
        let next_exp = exp * exp;
        // Perform FFT on even and odd sub-arrays.
        self.basic_fft(xs, transformed_xs, next_exp, start, step * 2, n / 2);
        self.basic_fft(xs, transformed_xs, next_exp, start + step, step * 2, n / 2);
    
        // X[k] = Xe[k] + e^(j*omega*k) Xo[k]           for 0 <= k < n / 2
        // X[k] = Xe[k] - e^(j*omega*k) Xo[k]           for 0 <= k < n / 2
        let mut current_exp = Complex::<f64>::new(1.0, 0.0);
        for i in 0..n / 2 {
            // If the branch is taken, Xe[2 * i] has already been overwritten.
            // As such it is stored in self.buffer. This case is also similar
            // for the odd term.
            let even_term = if i >= n / 4 {
                self.buffer[(2 * i) % (n / 4)]
            } else {
                transformed_xs[index(2 * i)]
            };
            let odd_term = if i >= n / 4 && i < n / 2 - 1 {
                self.buffer[(2 * i + 1) % (n / 4)] * current_exp
            } else {
                transformed_xs[index(2 * i + 1)] * current_exp
            };
            
            /* Save transformed value that is about to be overwritten. 
             * note: (i + n / 2) % (n / 4) == i % (n / 4) */
            self.buffer[i % (n / 4)] = transformed_xs[index(i + n / 2)];

            transformed_xs[index(i)] = even_term + odd_term;
            transformed_xs[index(i + n / 2)] = even_term - odd_term;
            current_exp = current_exp * exp;
        }
    }
    
    /// Performs an fft on a buffer 'xs' in the direction specified by 'dir'
    /// Returns a new vector containing the transformed buffer.
    pub fn fft(&mut self, xs: &Vec<Complex<f64>>, dir: Direction) -> Vec<Complex<f64>> {
        let len = xs.len();
        /* FFT is not designed for buffer size less than 1 */
        debug_assert!(len > 1);

        if self.buffer.len() < len / 4 {
            self.buffer.resize(len / 4, Complex::new(0.0, 0.0));
        }

        /* rads per sample */
        let angle =  2.0 * PI / (len as f64) * match dir {
            Direction::FORWARD   => -1.0,
            Direction::BACKWARD  =>  1.0,
        };
        let mut transformed_xs = vec![Complex::<f64>::new(0.0, 0.0); len];
        let exp = Complex::<f64>::from_polar(1.0, angle);

        self.basic_fft(xs, &mut transformed_xs, exp, 0, 1, len);
        transformed_xs
    }
   
    /// Divide buffer by it's length. Needed after an inverse fft.
    pub fn normalise(xs: &mut Vec<Complex<f64>>) {
        let len = xs.len();
        for i in 0..len {
            xs[i] /= len as f64;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::prelude::*;
    use approx::*;

    fn gen_rand_buffer(len: usize) -> Vec<Complex<f64>> {
        assert!(len.is_power_of_two());
        let mut xs = Vec::with_capacity(len);
        for _ in 0..len {
            xs.push(Complex::new(random(), random()));   
        }
        xs
    }
    
    fn fft_is_injective(len: usize) {
        let xs = gen_rand_buffer(len);
        let mut transformer = FFTransformer::new();
        let transformed_xs = transformer.fft(&xs, Direction::FORWARD);
        let mut xs_reverted = transformer.fft(&transformed_xs, Direction::BACKWARD);
        FFTransformer::normalise(&mut xs_reverted);
        for i in 0..xs.len() {
            /* Ensure values are within 0.1% of eachother. */
            assert_relative_eq!(xs[i].re, xs_reverted[i].re, max_relative = 0.001, epsilon = f64::EPSILON);
            assert_relative_eq!(xs[i].im, xs_reverted[i].im, max_relative = 0.001, epsilon = f64::EPSILON);
        }
    }

    #[test]
    fn fft_is_injective_many() {
        for _ in 0..100 {
            let n : u8 = random::<u8>() % 15;
            fft_is_injective(2 << n);
        }
    }

}

