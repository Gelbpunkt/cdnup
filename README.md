# cdnup

CDN upload server for the Travitia CDN.

## Database

This server uses SQLite due to being portable and having almost zero overhead.

```sh
cargo install --version=0.1.0-beta.1 sqlx-cli
export DATABASE_URL="sqlite:cdn.db"
sqlx db create
sqlx migrate run
```

## Note

If you run this externally exposed, set the env var `BASE_URL` to where your server runs.

This code was not made to run under Windows, which has completely different path behavior and might not work at all or pose security risks.

## API Usage

`POST /my-file.png`

Uploads a file from the request body and returns the path it will be available at.

`PUT /path/to/my-file.png`

Overwrites the content of an existing file and returns its path.

`DELETE /path/my-file.png`

Deletes the file.

`PATCH /path/to/my-file.png` with header `X-Rename-To` to specify a new filename.

Renames the file to something else.

All requests require the `Authorization` header to be set to something valid in the database.

## Running

```
podman build -t cdnup:latest .
podman run --rm -it -v /path/to/cdn.db:/cdn.db:Z -v /path/to/uploads:/uploads:Z -e UPLOAD_DIRECTORY="/uploads" -e BASE_URL="http://localhost:5006" -p 5006:5006 cdnup:latest
```
