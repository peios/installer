---
title: Network Choices during Setup
type: concept
description: Reading network state and applying a manual wired address at the end of setup.
---

Setup shows network state reported by `net status`. You can continue on
the console without a network, or configure a manual address for a wired
interface. Joining a wireless network is not part of this setup flow.

The network page reports each interface and its addresses, gateway, and
name servers. **Check again** refreshes that state. In a browser, the page
also identifies the interface used to reach the machine when it can match
the address.

A manual address is saved as a pending choice. It does not change the
connection while you complete the account and machine-name pages. Setup
applies it after creating the account and naming the machine.

The daemon writes an ordinary network profile and an interface rule under
`Machine\System\Network`. The rule identifies the interface by its stable
identifier. These settings belong to the network service and can be changed
through its normal tools after setup.

Before finishing, check that your browser's device can reach the address you
chose. If it cannot, use the console to finish and inspect the connection.
A browser that stays on its waiting page does not prove that the network
change failed.
