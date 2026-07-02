pub struct Auth;

impl Auth {
    pub fn login(&self, user: &str, pass: &str) -> Result<Token, AuthError> {
        if !self.check(user, pass) {
            std::thread::sleep(FIXED_DELAY);
            return Err(AuthError::BadCredentials);
        }
        Ok(self.issue(user))
    }

    pub fn logout(&self, token: Token) {
        self.revoke(token);
    }
}
