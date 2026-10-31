---
sidebar_position: 9
title: Kubernetes
description: "Create a local Kubernetes cluster in Ferrodock only when you need one: k3s in containers, with kubectl set up, worker nodes, an ingress port and local images loaded in one click."
---

# Kubernetes

The **Kubernetes** page, under **START** in the sidebar, makes a local
Kubernetes cluster when you ask for one, and only then. Until you create a
cluster, nothing runs and no memory is used, and a stopped cluster uses
none either.

A cluster is [k3s](https://k3s.io), the lightweight Kubernetes, running in
containers on your engine: a server node, plus worker nodes if you want
them. It starts in under a minute.

## Creating a cluster

Click **Create cluster…** (or **New cluster** once you have one). The
dialog asks for:

- **Name**, e.g. `dev`.
- **Kubernetes** version: 1.37, 1.36 or 1.35.
- **Worker nodes**: 0 to 3. The server runs your pods too, so 0 is fine for
  most work. Each node uses about 500 MB of memory while it runs.
- **Ingress port**: the port on your computer for the cluster's port 80,
  where its ingress (Traefik) answers. 8080 unless that's taken.
- **API port**: the port of the Kubernetes API, 6550 unless that's taken.
- **Use it with kubectl** (on by default): adds the context
  `ferrodock-<name>` to your kubeconfig and switches `kubectl` to it.

Ferrodock downloads k3s if it isn't on your machine yet, starts the nodes
and waits until they're ready. The cluster opens when it's up.

## Using kubectl and other tools

With **Use it with kubectl** on, `kubectl`, Helm, k9s, Lens and your IDE
on this computer reach the cluster as they would any other:

```bash
kubectl config use-context ferrodock-dev   # switch back to it later
kubectl get nodes
```

Ferrodock writes to the file in `KUBECONFIG`, or `~/.kube/config`. It only
adds the cluster's own context, user and cluster entries, and keeps a copy
of the file as it was next to it (`config.ferrodock-backup`). Removing the
cluster takes its context out again.

## Inside a cluster

**Open** shows the cluster:

- **Overview**: its nodes, and every pod with its state, restarts and node,
  updated every few seconds. Your own pods are listed first.
- **Terminal**: a shell on the server node, with `kubectl` ready.
- **Load image**: copies an image from Docker into every node, so pods can
  use images you built without pushing them to a registry. Give the pod
  `imagePullPolicy: IfNotPresent` (or `Never`).

The header has the kubectl context (with **Copy** for the command that
switches to it), the ingress address, and the API address.

## Reaching your apps

Port 80 of the cluster, where its Traefik ingress controller answers, is
on the ingress port of your computer. Give your app an `Ingress` and it's
at `http://localhost:8080` (or the port you picked):

```bash
kubectl create deployment web --image=nginx:alpine
kubectl expose deployment web --port=80
kubectl create ingress web --rule="/*=web:80"
# then open http://localhost:8080
```

## Stopping and removing

**Stop** (the square on its row) stops every node and frees its memory;
**Start** brings it back as it was, with your workloads and loaded images.
**Remove** (the ✕) deletes the nodes, their network and storage, and the
kubectl context, after asking.

:::tip
A cluster's nodes are containers named `<name>-server` and
`<name>-worker-1`…, so they show up on the **Containers** page too.
:::
