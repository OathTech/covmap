# HTTP API

This document specifies the four endpoints of the widget service.

## GET /widgets

Returns the list of all widgets as a JSON array. The list is never
paginated and is safe to cache for up to sixty seconds. No authentication
is required.

## POST /widgets

Creates a widget from the JSON body. Responds 201 with the new widget's
id in the Location header. A malformed body is rejected with 400. The
caller must present a valid bearer token; an unauthenticated request is
rejected with 401.

## DELETE /widgets/{id}

Removes the widget with the given id. Responds 204 on success and 404 if
no widget has that id. The operation is idempotent. The caller must
present a valid bearer token with the `widgets:delete` scope; otherwise
the request is rejected with 403.

## GET /healthz

A liveness probe. Always responds 200 with the body "ok" and performs no
database access, so it stays green during a backend outage.
