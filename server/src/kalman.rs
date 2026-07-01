//! Shared 1D constant-velocity Kalman filter (state: [position, velocity]).
//! Used by both the FRED yield-curve velocity series and the weight trend estimator,
//! which previously carried near-identical copies of this struct with different tuning.

pub struct KalmanFilter {
    x: [f64; 2],
    p: [[f64; 2]; 2],
    q: [[f64; 2]; 2],
    r: f64,
}

impl KalmanFilter {
    /// `q_position`/`q_velocity` are the process noise for position and velocity;
    /// `r` is the measurement noise. `initial_velocity` seeds the velocity estimate.
    pub fn new(
        initial_position: f64,
        initial_velocity: f64,
        q_position: f64,
        q_velocity: f64,
        r: f64,
    ) -> Self {
        Self {
            x: [initial_position, initial_velocity],
            p: [[1.0, 0.0], [0.0, 1.0]],
            q: [[q_position, 0.0], [0.0, q_velocity]],
            r,
        }
    }

    pub fn predict(&mut self, dt_days: f64) {
        // State transition: x = F * x, where F = [[1, dt], [0, 1]]
        let x0 = self.x[0] + self.x[1] * dt_days;
        let x1 = self.x[1];
        self.x = [x0, x1];

        // Covariance: P = F * P * F^T + Q
        let p00 = self.p[0][0]
            + 2.0 * dt_days * self.p[0][1]
            + dt_days * dt_days * self.p[1][1]
            + self.q[0][0];
        let p01 = self.p[0][1] + dt_days * self.p[1][1] + self.q[0][1];
        let p10 = p01;
        let p11 = self.p[1][1] + self.q[1][1];
        self.p = [[p00, p01], [p10, p11]];
    }

    pub fn update(&mut self, measurement: f64) {
        // Measurement model: H = [1, 0]
        let innovation = measurement - self.x[0];
        let innovation_covariance = self.p[0][0] + self.r;
        let k0 = self.p[0][0] / innovation_covariance;
        let k1 = self.p[1][0] / innovation_covariance;

        self.x[0] += k0 * innovation;
        self.x[1] += k1 * innovation;

        // Covariance update: P = (I - K * H) * P
        let p00 = (1.0 - k0) * self.p[0][0];
        let p01 = (1.0 - k0) * self.p[0][1];
        let p10 = self.p[1][0] - k1 * self.p[0][0];
        let p11 = self.p[1][1] - k1 * self.p[0][1];
        self.p = [[p00, p01], [p10, p11]];
    }

    pub fn position(&self) -> f64 {
        self.x[0]
    }

    pub fn velocity(&self) -> f64 {
        self.x[1]
    }

    pub fn position_variance(&self) -> f64 {
        self.p[0][0]
    }

    pub fn position_velocity_covariance(&self) -> f64 {
        self.p[0][1]
    }

    pub fn velocity_variance(&self) -> f64 {
        self.p[1][1]
    }
}
