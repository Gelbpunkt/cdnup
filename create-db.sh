#!/usr/bin/env bash
export DATABASE_URL="sqlite:cdn.db"
sqlx db create
sqlx migrate run
