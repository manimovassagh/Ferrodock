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

The containers page shows every container with its state, image, ID,
current CPU (with a short trend line, amber above 80%), memory and
published ports. Use **All / Running / Stopped** to filter.

- **▶ / ■** starts or stops a container, **⏸ / ⏵** pauses and resumes it,
  and **⟳** restarts it.
- Published ports are links: click **8080 ↗** to open the app in your
  browser, or right-click to copy its address.
- **Remove…** (in the details pane's **⋯** menu) is available once the
  container is stopped, and always asks for confirmation.

Changes show up the moment they happen, whether you make them in
Ferrodock, in a terminal or with Docker Compose. Ferrodock follows the
Engine's event stream. While an action runs, the status says what's
happening ("Stopping…", "Restarting…").

If a container **crashes** (exits with an error you didn't ask for) or is
killed for **running out of memory**, a notification tells you.

In a container's details, the **⋯** menu next to the actions has:

- **Rename…** gives it a new name.
- **Edit…** opens the Run dialog filled in with its current setup. Change
  ports, environment, volumes, network, limits or the command, then
  **Recreate** replaces the container with the new one. Named volumes and
  bind mounts are kept; changes made inside the container itself are not.
  If the new container can't start, the old one is put back as it was.
- **Duplicate…** opens the same dialog for a copy: the host ports move up by
  one so they don't clash.
- **Restart** (in Overview) changes its restart policy on the spot, without
  recreating it.
- IDs, the IP address and the command can be copied: click the value, or
  **Copy** next to it.
- Hover over the CPU or memory chart (in Stats) to read the value at any
  moment of the last minute.
- **Limits** (in Overview) shows its CPU and memory limits; **Change…** sets
  them on the spot. Removing a limit needs **Edit…**.

To act on several containers at once, **⌘-click** (Ctrl-click) rows to
select them, **Shift-click** to select a range, or press **⌘A** (Ctrl+A)
for all of them. A bar above the list then offers **Start**, **Stop**,
**Restart** and **Remove…** for the selection; **Esc** clears it.

### Logs

The **Logs** tab shows the last 500 lines, then follows new output live.

- Colours from the container (ANSI escape codes) are shown as colours.
  Lines without colours that mention an error or a warning are tinted red
  or amber.
- Type in **Filter logs** to show only matching lines; matches are
  highlighted.
- **Time** shows when each line was written; **Wrap** wraps long lines.
- Scroll up to read back; **Latest** jumps to the newest line and keeps
  following.
- **Copy** copies the shown lines, and **Save** writes the whole log to
  your Downloads folder.

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

Ferrodock starts in light mode. Switch to dark, or let it follow your
system, in [Settings](#settings).

![Dark mode](/img/dark-mode.png)

### Processes

The **Top** tab lists what's running inside the container (process ID,
user, CPU time and command), refreshed every two seconds while it's open.

## Pull and run images

On the **Images** page, type an image name such as `nginx:alpine` in the
**Pull** box and press Enter. A progress bar shows the download.

To start a container from an image, hover its row and click **▶**, or
select it and click **Run…**. The dialog asks for:

- **Name** (optional; Docker picks one otherwise).
- **Ports**, filled in from the ports the image exposes. Change the host
  side if it's taken.
- **Environment** variables, **Volumes** (a volume name or a host path),
  **Network**, **Restart** policy and an optional **Command**.

An image's details pane also has a **History** section listing the layers
it was built from, with the biggest ones highlighted, so you can see where
its size comes from.

The equivalent `docker run` command is shown as you type, with a Copy
button, so you can reuse it in scripts.

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

## Volumes and networks

**New volume** and **New network** create them (a network can get its own
subnet, or be *internal* with no internet access). In a network's details,
**Disconnect** takes a container off it, and the picker below
**Connect**s another one.

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

