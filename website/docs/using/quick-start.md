---
sidebar_position: 7
title: Quick start
description: "Start Postgres, Redis, MySQL and other popular services for local development in one click with Ferrodock's Quick start."
---

# Quick start

The **Quick start** page, under **START** in the sidebar, has cards for
popular services, set up for local development. Type in the filter box to
narrow the cards.

| Service | Image | Ports | Login |
|---|---|---|---|
| Postgres | `postgres:17` | 5432 | postgres / postgres, database `app` |
| MySQL | `mysql:8.4` | 3306 | root / mysql, database `app` |
| Redis | `redis:8` | 6379 | none |
| MongoDB | `mongo:8` | 27017 | mongo / mongo |
| RabbitMQ | `rabbitmq:4-management` | 5672 (AMQP), 15672 (web UI) | guest / guest |
| MinIO | `minio/minio` | 9000 (S3 API), 9001 (console) | minioadmin / minioadmin |
| Mailpit | `axllent/mailpit` | 1025 (SMTP), 8025 (web UI) | none |
| Nginx | `nginx:alpine` | 8080 → 80 | none |

## Running a service

Click **Run…** on a card. The dialog asks for:

- **Name** and **Version** (the image tag).
- **Ports** on your machine. If a port or the name is taken, the next free
  one is suggested.
- The service's own settings, such as user, password and database.
- **Keep data** (on by default) stores the data in a volume named
  `<name>-data`, so it survives removing the container.

The **connection string**, such as
`postgres://postgres:postgres@localhost:5432/app`, updates as you type.
Click **Copy** to copy it.

If the image isn't on your machine, Ferrodock pulls it first. The card
itself says how it's going: **Pulling… 42%**, then **Starting…**, then
**Running**. If it fails (say, the port is taken), the card says
**Couldn't start**; hover over it for the reason. The container restarts
by itself unless you stop it.

A card whose service already runs shows **Running**, and **Show** opens
that container.

## From the menu bar

The [menu bar or notification area icon](./settings.md#menu-bar-and-notification-area)
has a **Quick start** menu too: click a service to run it right away with
its defaults.

:::caution
These defaults are for local development only. Change the passwords for
anything else.
:::
