-- Add migration script here
CREATE TABLE IF NOT EXISTS users ("id" SERIAL PRIMARY KEY NOT NULL, "key" TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS uploads ("file_path" TEXT PRIMARY KEY NOT NULL, "upload_date" TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP, "uploader" integer REFERENCES users("id") NOT NULL);
