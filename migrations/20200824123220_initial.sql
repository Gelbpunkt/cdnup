-- Add migration script here
CREATE TABLE IF NOT EXISTS users
(
    "id" INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    "key" TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS uploads
(
    "file_path" TEXT PRIMARY KEY NOT NULL,
    "upload_date" TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    "uploader" INTEGER NOT NULL,
    FOREIGN KEY("uploader") REFERENCES users("id")
);
