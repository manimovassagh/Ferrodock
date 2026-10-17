---
sidebar_position: 3
title: Using Ferrodock
---

# Using Ferrodock

The sidebar lists every section with a count, and the Docker Engine status
at the bottom. Each page has a search box, and lists refresh every few
seconds. Press **F5** to refresh right away.

Click any row to open its **details pane** on the right. Click it again, or
the ✕, to close it.

## Containers

The containers page shows every container with its state, image, ID and
published ports. Use **All / Running / Stopped** to filter.

- **▶ / ■** starts or stops a container, **⏸ / ⏵** pauses and resumes it,
  and **⟳** restarts it.
- Published ports are links: click **8080 ↗** to open the app in your
  browser, or right-click to copy its address.
- **Remove** (in the details pane) is available once the container is
  stopped, and always asks for confirmation.

### Logs

The **Logs** tab shows the last 500 lines, then follows new output live. Use
the filter box to narrow it down, and **Copy** to copy the shown lines.

![Live logs](/img/logs.png)

### Stats

The **Stats** tab shows CPU and memory with a one-minute chart, plus network
and disk totals. CPU is measured like `docker stats`: 100% means one full
CPU core.

![Live stats](/img/stats.png)

Logs and stats stream only for the container whose details are open, so
Ferrodock stays light.

## Compose

Containers started by Docker Compose are grouped by project. A project shows
**Running**, **Running 2/3** (some services stopped) or **Stopped**.

- **Start all / Stop all / Restart all** act on the whole project.
- **Remove** deletes the project's containers once it's stopped. Volumes,
  networks and images are kept, and `docker compose up` recreates
  everything.

## Images, volumes and networks

Each page shows what's in use and by which containers.

- **Remove** in the details pane deletes one item. It's disabled, with the
  reason on hover, while something still uses it.
- **Remove unused** in the page header removes everything nothing uses.
  Docker's built-in networks (`bridge`, `host`, `none`) are never touched.

![Volumes](/img/volumes.png)

## Themes

Ferrodock follows your system's light or dark setting.

![Light mode](/img/light-mode.png)

## Inspect

Every details pane can show what `docker inspect` knows. Containers have an
**Inspect** tab; images, volumes and networks have an **Inspect** section at
the bottom.

- **Runtime**: the restart policy, and for containers with a healthcheck,
  the health status, how many checks failed in a row and the last output.
- **Environment**: every variable. Values that look like secrets
  (passwords, tokens, keys) are hidden until you tick **Show secrets**.
  Click a row to copy it as `KEY=value`.
- **Labels**, including the ones Docker Compose adds.
- **Raw JSON**: the full output, with a copy button.

## Disk

The **Disk** page shows how much space images, containers, volumes and the
build cache take, and how much of it can be freed. The total is also shown
in the sidebar.

Under **Clean up**, tick what to remove: stopped containers, untagged or all
unused images, unused volumes, unused networks, or the build cache. Ferrodock
lists exactly what will be deleted and asks before it does anything. When
it's done, a notification says how much space was freed.

Disk usage is loaded when you open the page or press refresh, never in the
background. It needs Docker Engine 29 or newer.

## Settings

Click **⚙ Settings** at the bottom of the sidebar, or press **⌘,** (Ctrl+,
on Windows and Linux).

- **Theme**: Light (the default), Dark, or System to follow your operating
  system.
- **Density**: Comfortable shows two lines per row; Compact fits about
  twice as many rows on screen.
- **Interface size**: 90% to 125%.

Settings, the window size and its position are remembered between runs.

## Keyboard shortcuts

Press **⌘K** (macOS) or **Ctrl+K** (Windows, Linux) to open the **command
palette**. Type a few letters of anything, such as a container, image,
volume, network, Compose project or action ("restart api", "remove unused
images"), and press **Enter**. Entries you used recently come first.

| Keys | Does |
|---|---|
| ⌘/Ctrl+K | Command palette |
| ⌘/Ctrl+, | Settings |
| ⌘/Ctrl+1 … 6 | Switch page |
| / | Search this page |
| ↑ ↓ or J K | Select the previous or next item |
| Esc | Close the details pane |
| S | Start or stop the selected container |
| R | Restart it |
| P | Pause or resume it |
| L | Show its logs |
| ⌫ or Delete | Remove it (asks first) |
| F5 | Refresh |
| ? | Show all shortcuts |

