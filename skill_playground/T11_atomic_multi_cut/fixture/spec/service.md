# Service Pipeline

The service processes each request through four ordered phases.

## Parse

Parse the request body and normalize its fields.

## Validate

Validate required fields and reject unsupported values.
Validation does not mutate persistent state.

## Execute

Execute the requested operation transactionally.

## Report

Return the result and record an audit event.
