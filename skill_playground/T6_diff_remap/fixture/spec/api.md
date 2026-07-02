# API Spec

The service exposes three endpoints over HTTP.

## Create

`POST /items` creates an item from the JSON body and returns its id.

## Read

`GET /items/{id}` returns the item, or 404 if no such id exists.

## Delete

`DELETE /items/{id}` removes the item and returns 204 on success.
