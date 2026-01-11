#!/bin/sh
set -e

echo "⏳ Waiting for PostgreSQL at db:5432..."
until nc -z db 5432; do
    sleep 1
done

FLAG_FILE=/app/.user_initialized

if [ ! -f "$FLAG_FILE" ]; then
    echo "👤 Running create_user (one-time init)"
    /app/create_user
    touch "$FLAG_FILE"
else
    echo "✅ create_user already executed"
fi

echo "🚀 Starting panel"
exec /app/panel
