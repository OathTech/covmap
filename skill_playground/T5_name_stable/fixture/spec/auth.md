# Auth Spec

The auth module issues and validates session tokens.

## Login

`login(user, pass)` checks the credentials and, on success, returns a
fresh session token. On failure it returns an error after a fixed delay.

## Logout

`logout(token)` invalidates the token immediately. Subsequent use of the
token is rejected.

## Expiry

Tokens expire after thirty minutes of inactivity. An expired token is
treated as invalid.
