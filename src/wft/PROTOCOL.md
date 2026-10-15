# WiFi File Transfer (WFT) Android app HTTP API

This specification describes the HTTP API exposed by the WiFi File Transfer Android app. The API
uses HTTP with JSON responses on the default port `1234`. Endpoints use `GET` requests and require
no custom request headers.

## Endpoints

- ### `GET /api/ping`

Checks that the server is reachable. A successful response has the plain-text body `pong`.

```text
pong
```

- ### `GET /api/get-platform-info`

Returns platform information as a JSON object with a `platform` string.

```json
{"platform":"android"}
```

- ### `GET /api/check-manage-storage-permission`

Reports whether the app has Android's manage-external-storage permission. The response is a JSON
object with the boolean field `isExternalStorageManager`.

```json
{"isExternalStorageManager":true}
```

- ### `GET /api/get-android-version`

Returns Android version information. `SDK_INT` is a number and `RELEASE` is a string.

```json
{"SDK_INT":33,"RELEASE":"13"}
```

- ### `GET /api/get-storage-details`

Returns a JSON array containing storage summaries. Each summary has string fields `path`, `name`,
and `rootPath`, and numeric fields `size`, `totalBytes`, and `availableBytes`. Storage quantities
are in bytes. Example:

```json
[
  {
    "path": "/storage/emulated/0/DCIM",
    "size": 45441988872,
    "name": "DCIM",
    "totalBytes": 117673275392,
    "rootPath": "/storage/emulated/0",
    "availableBytes": 21686992896
  }
]
```

- ### `GET /api/directory/{absolute-path}`

Lists the directory identified by the absolute storage path. Append the path directly after
`/api/directory/`; for example, `/storage/emulated/0` is requested as
`/api/directory/storage/emulated/0`. Do not add a `root` path component.

The response is a JSON object with `directories` and `files` arrays. Each entry has string fields
`name`, `path`, and `extension`, and numeric fields `size` and `modified`. The units and semantics
of `size` and `modified` are not specified. The current client treats an omitted `directories` or
`files` field as an empty array. Example:

```json
{
  "directories": [
    {
      "name": "DCIM",
      "size": 3488,
      "path": "/storage/emulated/0/DCIM",
      "modified": 1698271261496,
      "extension": ""
    }
  ],
  "files": []
}
```

## Errors and headers

Successful API responses use `200 OK` and `Content-Type: application/json; charset=UTF-8`.
Error responses may also use `200 OK`; callers must inspect the JSON body. An observed error body
has the form `{"error": string}`. For example:

```json
{"error":"path: /root/storage/emulated/0 is not a directory"}
```
