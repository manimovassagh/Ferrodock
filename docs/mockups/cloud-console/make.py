"""Builds the console mockups: one frame, five screens."""
import pathlib

HERE = pathlib.Path(__file__).parent

ICONS = {
    "bucket": "M4 7c0-1.7 3.6-3 8-3s8 1.3 8 3-3.6 3-8 3-8-1.3-8-3zm0 0 1.6 11.2c.2 1.6 3 2.8 6.4 2.8s6.2-1.2 6.4-2.8L20 7",
    "queue": "M3 7h12M3 12h12M3 17h8m6-10 4 5-4 5",
    "grid": "M4 4h6v6H4zM14 4h6v6h-6zM4 14h6v6H4zM14 14h6v6h-6z",
    "search": "M11 18a7 7 0 1 0 0-14 7 7 0 0 0 0 14zm5-2 4 4",
    "cloud": "M7 18h10.5a4.5 4.5 0 0 0 .6-8.96A6 6 0 0 0 6.6 9.5 4.25 4.25 0 0 0 7 18Z",
    "refresh": "M20 11a8 8 0 1 0-2.3 5.7M20 4v7h-7",
    "gear": "M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-2.9 1.2V21a2 2 0 1 1-4 0v-.1A1.7 1.7 0 0 0 7 19.4l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1A1.7 1.7 0 0 0 3 13.7H3a2 2 0 1 1 0-4h.1A1.7 1.7 0 0 0 4.6 7l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1A1.7 1.7 0 0 0 9 3.1V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 2.9 1.2l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1A1.7 1.7 0 0 0 20.9 9H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 2z",
    "info": "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zm0-13h.01M11 11h1v5h1",
    "warn": "M12 3 2 20h20zm0 6v5m0 3h.01",
    "check": "M5 12l5 5 9-10",
    "x": "M6 6l12 12M18 6 6 18",
    "copy": "M9 9h10v10H9zM5 15V5h10",
    "send": "M4 12 20 4l-6 16-3-7z",
    "plus": "M12 5v14M5 12h14",
    "ext": "M14 4h6v6M20 4l-9 9M18 14v6H4V6h6",
}


def icon(name, size=16):
    return f'<svg class="i" viewBox="0 0 24 24" style="width:{size}px;height:{size}px"><path d="{ICONS[name]}"/></svg>'


def tile(name, gradient, size=26):
    return (f'<span style="display:inline-grid;place-items:center;width:{size}px;height:{size}px;border-radius:6px;'
            f'background:{gradient};color:#fff">{icon(name, int(size*0.6))}</span>')


GREEN = "linear-gradient(135deg,#2e8b57,#7ac143)"
PINK = "linear-gradient(135deg,#b0084d,#ff4f8b)"


def frame(title, side, content, extra="", narrow=False, dark=False):
    nav = f"""
<nav class="nav">
  <span class="logo"><span class="mark">{icon('cloud', 15)}</span>ferrocloud <small>AWS emulator</small></span>
  <span class="item">{icon('grid')} Services</span>
  <span class="search">{icon('search')} Search services, buckets, queues, roles…<kbd>⌘ K</kbd></span>
  <span class="right">
    <span class="item"><span class="dot"></span> Running</span>
    <span class="item">us-east-1 ▾</span>
    <span class="item">Account 0000-0000-0000 ▾</span>
  </span>
</nav>"""
    side_html = f'<nav class="side">{side}</nav>' if side else ""
    return f"""<!doctype html><html lang="en"><head><meta charset="utf-8"><title>{title}</title>
<link rel="stylesheet" href="shell.css"></head><body class="{'dark' if dark else ''}">{nav}
<div class="layout">{side_html}<main class="content{' narrow' if narrow else ''}">{content}</main></div>{extra}
<div class="label">{title}</div></body></html>"""


S3_SIDE = f"""<h3>{tile('bucket', GREEN, 24)} S3</h3>
<a class="on">Buckets</a><a>Access points</a><a>Storage Lens</a>
<hr><div class="group">Emulator</div><a>Requests to S3</a><a>Settings {icon('ext', 13)}</a>"""

SQS_SIDE = f"""<h3>{tile('queue', PINK, 24)} SQS</h3>
<a class="on">Queues</a><a>Dead-letter queues</a>
<hr><div class="group">Emulator</div><a>Requests to SQS</a>"""


def buckets(menu=False, dark=False):
    rows = [
        ("shop-assets", "us-east-1", "1,284", "486.2 MB", "Off", "November 14, 2026, 22:22", True),
        ("order-exports", "us-east-1", "12", "1.4 MB", "Enabled", "November 14, 2026, 22:21", False),
        ("user-uploads", "us-east-1", "3,907", "2.1 GB", "Off", "November 13, 2026, 09:05", False),
        ("terraform-state", "us-east-1", "4", "88 KB", "Enabled", "November 12, 2026, 17:40", False),
        ("logs-archive", "us-east-1", "0", "0 B", "Suspended", "November 10, 2026, 11:12", False),
    ]
    trs = "".join(
        f"""<tr class="{'sel' if sel else ''}"><td><span class="radio {'on' if sel else ''}"></span></td>
<td><a>{n}</a></td><td>{r}</td><td>{o}</td><td>{s}</td>
<td><span class="status {'ok' if v=='Enabled' else 'info' if v=='Suspended' else ''}">{icon('check',14) if v=='Enabled' else ''}{v}</span></td><td>{c}</td></tr>"""
        for n, r, o, s, v, c, sel in rows
    )
    actions_menu = """<div class="menu" style="right:176px;top:62px">
<div>Upload</div><div>Copy URL</div><div>Edit versioning</div><hr><div>Empty</div><div class="hl" style="color:var(--bad)">Delete</div></div>""" if menu else ""
    content = f"""
<div class="flash ok">{icon('check')} Created bucket "terraform-state". <a style="color:#fff;text-decoration:underline">View details</a><span class="x">{icon('x')}</span></div>
<div class="crumbs"><a>Console Home</a><span class="sep">›</span><a>S3</a><span class="sep">›</span><span>Buckets</span></div>
<section class="container" style="position:relative">
  <header>
    <div><h2>Buckets <span class="count">(5)</span> <span class="info">Info</span></h2>
    <p>Buckets are containers for objects stored in S3. Everything here lives on your computer.</p></div>
    <div class="actions">
      <span class="icon-btn">{icon('refresh')}</span>
      <span class="btn normal">{icon('copy')} Copy ARN</span>
      <span class="btn normal">Actions <span class="caret">▾</span></span>
      <span class="btn primary">Create bucket</span>
    </div>
  </header>
  <div class="toolbar">
    <span class="filter">{icon('search')} Find buckets by name</span>
    <span class="pager">‹ <span class="cur">1</span> ›<span class="gear">{icon('gear', 18)}</span></span>
  </div>
  <table>
    <thead><tr><th style="width:40px"></th><th>Name <span class="sort">▲</span></th><th>Region</th><th>Objects</th><th>Size</th><th>Versioning</th><th>Creation date <span class="sort">▼</span></th></tr></thead>
    <tbody>{trs}</tbody>
  </table>
  {actions_menu}
</section>
<section class="container">
  <header><div><h2>Account snapshot</h2><p>Storage used across the emulator's buckets.</p></div><div class="actions"><span class="btn normal">View Storage Lens</span></div></header>
  <div class="body divided" style="padding-top:16px"><dl class="kv">
    <div><dt>Total storage</dt><dd class="big">2.6 GB</dd></div>
    <div><dt>Object count</dt><dd class="big">5,207</dd></div>
    <div><dt>Buckets</dt><dd class="big">5</dd></div>
    <div><dt>Requests today</dt><dd class="big">1,948</dd></div>
  </dl></div>
</section>"""
    return frame("S3 buckets: table with row selection, actions and paging", S3_SIDE, content, dark=dark)


def create_bucket():
    content = f"""
<div class="crumbs"><a>Console Home</a><span class="sep">›</span><a>S3</a><span class="sep">›</span><a>Buckets</a><span class="sep">›</span><span>Create bucket</span></div>
<div class="page-head"><div><h1>Create bucket <span class="info">Info</span></h1><p>Buckets hold objects. The bucket lives in the emulator, on this computer.</p></div></div>
<section class="container">
  <header><div><h2>General configuration</h2></div></header>
  <div class="body">
    <div class="field"><label>Region</label><div class="input half select" style="background:var(--soft)">US East (N. Virginia) us-east-1 <span class="muted">▾</span></div></div>
    <div class="field"><label>Bucket name <span class="info">Info</span></label>
      <div class="input bad half">Shop_Assets</div>
      <div class="error">{icon('warn',14)} Bucket names can use only lowercase letters, numbers, dots (.) and hyphens (-).</div>
      <div class="constraint">3 to 63 characters, starting and ending with a letter or number. Must be unique in this emulator.</div></div>
    <div class="field" style="margin-bottom:0"><label>Copy settings from an existing bucket <span class="muted" style="font-weight:400">- optional</span></label>
      <div class="hint">Only the bucket settings in the following configuration are copied.</div>
      <span class="btn normal">Choose bucket</span></div>
  </div>
</section>
<section class="container">
  <header><div><h2>Bucket Versioning <span class="info">Info</span></h2><p>Keep every version of an object, to restore it after it's overwritten or deleted.</p></div></header>
  <div class="body">
    <div style="display:flex;flex-direction:column;gap:8px"><span><span class="radio on"></span> &nbsp;Disable</span><span><span class="radio"></span> &nbsp;Enable</span></div>
  </div>
</section>
<section class="container">
  <header><div><h2>Tags <span class="muted" style="font-weight:400;font-size:14px">- optional</span> <span class="info">Info</span></h2><p>You can use bucket tags to track storage cost and organise buckets.</p></div></header>
  <div class="body"><div style="display:grid;grid-template-columns:1fr 1fr auto;gap:12px;align-items:end">
    <div class="field" style="margin:0"><label>Key</label><div class="input">env</div></div>
    <div class="field" style="margin:0"><label>Value <span class="muted" style="font-weight:400">- optional</span></label><div class="input">dev</div></div>
    <span class="btn normal">Remove</span></div>
    <div style="margin-top:12px"><span class="btn normal">Add tag</span> <span class="muted" style="margin-left:8px">You can add 49 more tags.</span></div></div>
</section>
<div class="alert">{icon('info',18)}<div>After creating the bucket, you can upload files and folders to it, or copy them in with <span class="mono">aws s3 cp</span>.</div></div>
<div class="form-actions"><span class="btn link">Cancel</span><span class="btn disabled">Create bucket</span></div>"""
    return frame("Create bucket: a full page, checked as you type", S3_SIDE, content, narrow=True)


def create_queue():
    content = f"""
<div class="crumbs"><a>Console Home</a><span class="sep">›</span><a>SQS</a><span class="sep">›</span><a>Queues</a><span class="sep">›</span><span>Create queue</span></div>
<div class="page-head"><div><h1>Create queue</h1></div></div>
<section class="container">
  <header><div><h2>Details</h2></div></header>
  <div class="body">
    <div class="field"><label>Type</label><div class="hint">Choose the queue type for your application. You can't change it after the queue is created.</div>
      <div class="tiles">
        <div class="tile-choice on"><span class="radio on"></span><div><b>Standard <span class="info">Info</span></b><small>At-least-once delivery, message ordering isn't preserved.</small>
          <ul style="margin:8px 0 0;padding-left:18px;color:var(--muted);font-size:13px"><li>At-least once delivery</li><li>Best-effort ordering</li></ul></div></div>
        <div class="tile-choice"><span class="radio"></span><div><b>FIFO <span class="info">Info</span></b><small>First-in-first-out delivery, message ordering is preserved.</small>
          <ul style="margin:8px 0 0;padding-left:18px;color:var(--muted);font-size:13px"><li>First-in-first-out delivery</li><li>Exactly-once processing</li></ul></div></div>
      </div></div>
    <div class="field" style="margin:0"><label>Name</label><div class="input focus half">orders-queue</div>
      <div class="constraint">A queue name is case-sensitive and can have up to 80 characters: letters, numbers, hyphens (-) and underscores (_).</div></div>
  </div>
</section>
<section class="container">
  <header><div><h2>Configuration <span class="info">Info</span></h2><p>Set the visibility timeout, message retention period and delivery delay of your queue.</p></div></header>
  <div class="body"><div style="display:grid;grid-template-columns:1fr 1fr;gap:4px 32px">
    <div class="field"><label>Visibility timeout <span class="info">Info</span></label><div style="display:flex;gap:8px"><div class="input" style="width:110px">30</div><div class="input select" style="width:130px">Seconds <span class="muted">▾</span></div></div><div class="constraint">Should be between 0 seconds and 12 hours.</div></div>
    <div class="field"><label>Message retention period <span class="info">Info</span></label><div style="display:flex;gap:8px"><div class="input" style="width:110px">4</div><div class="input select" style="width:130px">Days <span class="muted">▾</span></div></div><div class="constraint">Should be between 1 minute and 14 days.</div></div>
    <div class="field"><label>Delivery delay <span class="info">Info</span></label><div style="display:flex;gap:8px"><div class="input" style="width:110px">0</div><div class="input select" style="width:130px">Seconds <span class="muted">▾</span></div></div><div class="constraint">Should be between 0 seconds and 15 minutes.</div></div>
    <div class="field"><label>Receive message wait time <span class="info">Info</span></label><div style="display:flex;gap:8px"><div class="input" style="width:110px">0</div><span class="muted" style="align-self:center">Seconds</span></div><div class="constraint">Should be between 0 and 20 seconds.</div></div>
  </div></div>
</section>
<section class="container">
  <div class="expand" style="border-top:0">▾ Dead-letter queue <span class="muted" style="font-weight:400">- optional</span> <span class="info">Info</span></div>
  <div class="body">
    <div class="hint" style="color:var(--muted);margin-bottom:12px">Send messages that can't be processed to a dead-letter queue.</div>
    <div style="display:flex;flex-direction:column;gap:8px;margin-bottom:16px"><span><span class="radio"></span> &nbsp;Disabled</span><span><span class="radio on"></span> &nbsp;Enabled</span></div>
    <div style="display:grid;grid-template-columns:2fr 1fr;gap:24px">
      <div class="field" style="margin:0"><label>Choose queue</label><div class="input select">orders-dlq <span class="muted">▾</span></div></div>
      <div class="field" style="margin:0"><label>Maximum receives</label><div class="input">3</div><div class="constraint">Between 1 and 1,000.</div></div>
    </div>
  </div>
</section>
<section class="container"><div class="expand" style="border-top:0">▸ Tags <span class="muted" style="font-weight:400">- optional</span></div></section>
<div class="form-actions"><span class="btn link">Cancel</span><span class="btn primary">Create queue</span></div>"""
    return frame("Create queue: type cards, configuration, dead-letter queue", SQS_SIDE, content, narrow=True)


def delete_modal(dark=False):
    modal = f"""<div class="backdrop"><div class="modal">
<header><h2>Delete bucket "shop-assets"?</h2><span class="x">{icon('x',20)}</span></header>
<div class="body">
  <div class="alert bad" style="margin-bottom:14px">{icon('warn',18)}<div><b>This bucket isn't empty.</b> Deleting it deletes its 1,284 objects (486.2 MB) as well. This can't be undone.</div></div>
  <p style="margin:0 0 12px">To confirm, type <b>shop-assets</b> in the field.</p>
  <div class="input focus">shop-asse<span style="border-left:2px solid var(--text);margin-left:1px"></span></div>
</div>
<footer><span class="btn link">Cancel</span><span class="btn disabled">Delete bucket</span></footer>
</div></div>"""
    page = buckets().split('<main class="content">')[1].split("</main>")[0]
    return frame("Delete: a dialog in the page, typing the name to confirm", S3_SIDE, page, extra=modal, dark=dark)


def queue_page(dark=False):
    msgs = [
        ("22:41:22", '{"order": 1046, "total": 102.90, "items": 6}', "Available", "0"),
        ("22:41:21", '{"order": 1045, "total": 85.90, "items": 5}', "Available", "0"),
        ("22:41:20", '{"order": 1044, "total": 68.90, "items": 4}', "In flight", "1"),
        ("22:41:18", '{"order": 1043, "total": 51.90, "items": 3}', "Available", "0"),
    ]
    trs = "".join(f"""<tr><td><span class="check"></span></td><td class="mono">{t}</td><td class="mono">{b}</td>
<td><span class="status {'ok' if s=='Available' else 'info'}">{'●' if s=='Available' else '◐'} {s}</span></td><td>{r}</td></tr>""" for t, b, s, r in msgs)
    content = f"""
<div class="crumbs"><a>Console Home</a><span class="sep">›</span><a>SQS</a><span class="sep">›</span><a>Queues</a><span class="sep">›</span><span>orders-queue</span></div>
<div class="page-head"><div><h1>orders-queue</h1></div>
  <div class="actions"><span class="btn normal">Edit</span><span class="btn normal">Delete</span><span class="btn normal">Purge</span><span class="btn primary">Send and receive messages</span></div></div>
<section class="container">
  <header><div><h2>Details <span class="info">Info</span></h2></div></header>
  <div class="body divided" style="padding-top:16px"><dl class="kv">
    <div><dt>Name</dt><dd>orders-queue</dd></div>
    <div><dt>Type</dt><dd>Standard</dd></div>
    <div><dt>ARN</dt><dd class="mono">arn:aws:sqs:us-east-1:000000000000:orders-queue</dd></div>
    <div><dt>URL</dt><dd class="mono">http://localhost:4566/000000000000/orders-queue <a>{icon('copy',14)}</a></dd></div>
    <div><dt>Messages available</dt><dd class="big">5</dd></div>
    <div><dt>Messages in flight</dt><dd class="big">1</dd></div>
    <div><dt>Messages delayed</dt><dd class="big">0</dd></div>
    <div><dt>Dead-letter queue</dt><dd><a>orders-dlq</a> · after 3 receives</dd></div>
  </dl></div>
</section>
<div class="tabs"><div>Monitoring</div><div class="on">Messages</div><div>Dead-letter queue</div><div>Access policy</div><div>Tags</div></div>
<section class="container">
  <header><div><h2>Messages <span class="count">(6)</span> <span class="info">Info</span></h2><p>Looked at without receiving them: nothing is hidden or counted as received.</p></div>
    <div class="actions"><span class="icon-btn">{icon('refresh')}</span><span class="btn normal">Delete</span><span class="btn normal">Send message</span></div></header>
  <div class="toolbar"><span class="filter">{icon('search')} Find messages</span><span class="pager">‹ <span class="cur">1</span> ›<span class="gear">{icon('gear', 18)}</span></span></div>
  <table><thead><tr><th style="width:40px"><span class="check"></span></th><th>Sent <span class="sort">▼</span></th><th>Body</th><th>State</th><th>Receive count</th></tr></thead><tbody>{trs}</tbody></table>
</section>"""
    return frame("A queue: details, tabs, and its messages", SQS_SIDE, content, dark=dark)


PAGES = {
    "1-buckets": buckets(menu=True),
    "2-create-bucket": create_bucket(),
    "3-create-queue": create_queue(),
    "4-delete-modal": delete_modal(),
    "5-queue": queue_page(),
    "6-queue-dark": queue_page(dark=True),
}
for name, html in PAGES.items():
    (HERE / f"{name}.html").write_text(html)
print("\n".join(PAGES))
