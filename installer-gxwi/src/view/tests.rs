use super::*;
use serde_json::json;

/// installerd's first page, as it sends it (`installerd/src/flow.rs`).
fn mode() -> Turn {
    serde_json::from_value(json!({
        "seq": 1, "id": "mode", "name": "Peios Setup", "class": ["menu"],
        "elements": [
            { "ref": "mode.intro", "type": "text", "text": "Set up Peios on this machine." },
            { "ref": "act.install", "type": "action", "name": "Install Peios", "primary": true },
            { "ref": "act.upgrade", "type": "action", "name": "Upgrade an installation" },
            { "ref": "act.repair", "type": "action", "name": "Repair an existing system",
              "enabled": false, "help": "Not on this medium." },
        ],
    }))
    .unwrap()
}

/// installerd's disk page, for an install, with one disk to choose and
/// the medium it booted from.
fn disks() -> Turn {
    serde_json::from_value(json!({
        "seq": 2, "id": "disk.choose", "name": "Choose a disk", "class": ["install"],
        "elements": [
            { "ref": "disk.intro", "type": "text", "text": "Choose the disk to install onto." },
            { "ref": "disk.target", "type": "table", "name": "Target disk", "required": true,
              "help": "A storage controller that nothing on this system is driving.",
              "empty": "No disks found. Attach one and rescan.",
              "columns": [{ "key": "device", "name": "Device" }],
              "rows": [
                { "value": "/dev/sda", "note": "removable",
                  "cells": { "device": "/dev/sda", "model": "CT500MX500SSD1", "size": "465.8 GiB", "bus": "SATA" },
                  "detail": { "bytes": 500107862016u64, "partitions": [] } },
                { "value": "/dev/sdb", "enabled": false, "note": "boot medium",
                  "cells": { "device": "/dev/sdb", "model": "SanDisk Ultra Fit", "size": "28.6 GiB", "bus": "USB" } },
              ] },
            { "ref": "act.rescan", "type": "action", "name": "Rescan disks", "validate": false },
            { "ref": "disk.custom", "type": "action", "name": "Custom partitioning…",
              "enabled": false, "help": "Whole-disk only for now." },
            { "ref": "nav.back", "type": "action", "name": "Back", "validate": false },
            { "ref": "nav.next", "type": "action", "name": "Next", "primary": true },
        ],
    }))
    .unwrap()
}

#[test]
fn the_first_page_is_drawn_from_what_installerd_sends() {
    let Page::Mode {
        title,
        intro,
        actions,
    } = Page::of(&mode(), None)
    else {
        panic!("not the mode page")
    };
    assert_eq!(title, "Peios Setup");
    assert_eq!(intro, "Set up Peios on this machine.");
    assert_eq!(
        actions.iter().map(|a| a.r#ref.as_str()).collect::<Vec<_>>(),
        ["act.install", "act.upgrade", "act.repair"]
    );
    assert_eq!(
        actions.iter().map(|a| a.primary).collect::<Vec<_>>(),
        [true, false, false]
    );
    assert!(actions[0].enabled && actions[0].help.is_none());
    assert!(!actions[2].enabled);
    assert_eq!(actions[2].help.as_deref(), Some("Not on this medium."));
    // Every one of them leads to the disk page, which is drawn.
    assert!(actions.iter().all(|a| !a.unbuilt));
}

#[test]
fn the_disk_page_is_drawn_from_what_installerd_sends() {
    let Page::Disk {
        title,
        intro,
        purpose,
        disks,
        empty,
        trouble,
        error,
        chosen,
        rescan,
        custom,
        back,
        next,
    } = Page::of(&disks(), Some("/dev/sda"))
    else {
        panic!("not the disk page")
    };
    assert_eq!(
        (title.as_str(), intro.as_str(), purpose.as_str()),
        (
            "Choose a disk",
            "Choose the disk to install onto.",
            "install"
        )
    );
    assert_eq!(empty, "No disks found. Attach one and rescan.");
    assert_eq!(
        trouble.as_deref(),
        Some("A storage controller that nothing on this system is driving.")
    );
    assert_eq!(error, None);
    assert_eq!(chosen.as_deref(), Some("/dev/sda"));
    assert_eq!(
        disks[0],
        Disk {
            value: "/dev/sda".into(),
            device: "/dev/sda".into(),
            model: "CT500MX500SSD1".into(),
            size: "465.8 GiB".into(),
            bus: "SATA".into(),
            note: "removable".into(),
            enabled: true,
            detail: json!({ "bytes": 500107862016u64, "partitions": [] }),
        }
    );
    // The medium is listed and cannot be chosen, and an installerd that
    // sends no detail is drawn without any.
    assert!(!disks[1].enabled);
    assert_eq!(disks[1].note, "boot medium");
    assert_eq!(disks[1].detail, Value::Null);
    assert_eq!(rescan.unwrap().name, "Rescan disks");
    let custom = custom.unwrap();
    assert!(!custom.enabled);
    assert_eq!(custom.help.as_deref(), Some("Whole-disk only for now."));
    assert!(!back.unwrap().unbuilt);
    let next = next.unwrap();
    assert!(next.primary && next.enabled && !next.destructive);
    // An install goes on to the confirmation, which is drawn.
    assert!(!next.unbuilt);
}

/// installerd's confirmation of an install, with the disk it is about.
fn confirm() -> Turn {
    serde_json::from_value(json!({
        "seq": 3, "id": "confirm", "name": "Ready to install", "class": ["confirm"],
        "elements": [
            { "ref": "confirm.summary", "type": "text",
              "text": "Peios will be installed onto CT500MX500SSD1, 465.8 GiB (/dev/sda). The whole disk will be erased.",
              "detail": { "device": "/dev/sda", "model": "CT500MX500SSD1", "size": "465.8 GiB", "bus": "SATA",
                          "bytes": 500107862016u64, "partitions": [], "erases": [] } },
            { "ref": "nav.back", "type": "action", "name": "Back", "validate": false },
            { "ref": "act.begin", "type": "action", "name": "Erase disk and install", "primary": true, "destructive": true },
        ],
    }))
    .unwrap()
}

#[test]
fn the_confirmation_is_drawn_from_what_installerd_sends() {
    let Page::Confirm {
        title,
        summary,
        disk,
        back,
        begin,
    } = Page::of(&confirm(), None)
    else {
        panic!("not the confirmation")
    };
    assert_eq!(title, "Ready to install");
    assert!(summary.starts_with("Peios will be installed onto CT500MX500SSD1"));
    let disk = disk.unwrap();
    assert_eq!(
        (
            disk.value.as_str(),
            disk.device.as_str(),
            disk.model.as_str(),
            disk.size.as_str(),
            disk.bus.as_str()
        ),
        (
            "/dev/sda",
            "/dev/sda",
            "CT500MX500SSD1",
            "465.8 GiB",
            "SATA"
        )
    );
    assert_eq!(disk.detail["bytes"], 500107862016u64);
    assert_eq!(back.unwrap().r#ref, "nav.back");
    let begin = begin.unwrap();
    assert_eq!(begin.name, "Erase disk and install");
    assert!(begin.primary && begin.destructive && begin.enabled);
    // It leads to the installation as it runs, which is drawn.
    assert!(!begin.unbuilt);
    // Nothing of it is answered with but the action.
    assert!(values(&confirm(), Some("/dev/sda")).is_empty());
    assert_eq!(
        serde_json::to_value(Page::of(&confirm(), None)).unwrap()["kind"],
        "confirm"
    );
}

#[test]
fn a_confirmation_that_says_it_only_in_words_is_drawn_without_the_disk() {
    let mut turn = confirm();
    turn.elements[0].state.remove("detail");
    assert!(
        matches!(Page::of(&turn, None), Page::Confirm { disk: None, summary, .. } if !summary.is_empty())
    );
    // Nor is a disk made of facts that do not say which disk.
    turn.elements[0]
        .state
        .insert("detail".into(), json!({ "bytes": 1 }));
    assert!(matches!(
        Page::of(&turn, None),
        Page::Confirm { disk: None, .. }
    ));
}

#[test]
fn a_page_come_back_to_arrives_with_its_disk_chosen() {
    let mut turn = disks();
    assert_eq!(assumed(&turn), None);
    turn.elements[1].default = Some(json!("/dev/sda"));
    assert_eq!(assumed(&turn).as_deref(), Some("/dev/sda"));
    // Only a disk that is there to be chosen.
    turn.elements[1].default = Some(json!("/dev/sdb"));
    assert_eq!(assumed(&turn), None);
    turn.elements[1].default = Some(json!("/dev/nvme0n1"));
    assert_eq!(assumed(&turn), None);
    assert_eq!(assumed(&mode()), None);
}

#[test]
fn what_a_disk_is_chosen_for_is_read_from_the_page_not_guessed() {
    let mut turn = disks();
    turn.class = vec!["something".into(), "repair".into()];
    assert!(matches!(Page::of(&turn, None), Page::Disk { purpose, .. } if purpose == "repair"));
    turn.class.clear();
    assert!(matches!(Page::of(&turn, None), Page::Disk { purpose, .. } if purpose.is_empty()));
}

#[test]
fn only_a_disk_that_is_offered_can_be_chosen() {
    assert!(can_choose(&disks(), "/dev/sda"));
    // The boot medium is listed, and is not a choice.
    assert!(!can_choose(&disks(), "/dev/sdb"));
    assert!(!can_choose(&disks(), "/dev/nvme0n1"));
    assert!(!can_choose(&mode(), "/dev/sda"));
}

#[test]
fn the_disk_chosen_is_answered_with_on_the_page_that_asks_for_one() {
    assert_eq!(
        Value::Object(values(&disks(), Some("/dev/sda"))),
        json!({ "disk.target": "/dev/sda" })
    );
    assert!(values(&disks(), None).is_empty());
    assert!(values(&mode(), Some("/dev/sda")).is_empty());
}

#[test]
fn an_action_that_leads_to_a_page_not_drawn_is_not_sent_on() {
    // What an upgrade or a repair goes on to from the disks is not drawn
    // yet, nor what a disk page that does not say what it is for goes
    // on to.
    let mut turn = disks();
    assert!(!unbuilt(&turn, "nav.next"));
    for purpose in ["upgrade", "repair"] {
        turn.class = vec![purpose.into()];
        assert!(unbuilt(&turn, "nav.next"));
    }
    turn.class.clear();
    assert!(unbuilt(&turn, "nav.next"));
    assert!(!unbuilt(&turn, "nav.back"));
    assert!(!unbuilt(&turn, "act.rescan"));
    assert!(!unbuilt(&mode(), "act.install"));
    // The installation as it runs is drawn, so its button is sent on.
    assert!(!unbuilt(&confirm(), "act.begin"));
    assert!(!unbuilt(&confirm(), "nav.back"));
}

/// installerd's page for an install under way: the copy part done, and
/// three hundred lines said.
fn installing() -> Turn {
    serde_json::from_value(json!({
        "seq": 4, "id": "install.progress", "name": "Installing", "class": ["progress"],
        "elements": [
            { "ref": "progress.summary", "type": "text",
              "text": "Installing Peios onto CT500MX500SSD1, 465.8 GiB (/dev/sda). Leave the machine on.",
              "detail": { "device": "/dev/sda", "model": "CT500MX500SSD1", "size": "465.8 GiB", "bus": "SATA",
                          "bytes": 500107862016u64, "partitions": [], "becomes": [], "erases": [] } },
            { "ref": "phase.partition", "type": "progress", "name": "Partitioning", "value": 100, "max": 100 },
            { "ref": "phase.format", "type": "progress", "name": "Formatting", "value": 100, "max": 100 },
            { "ref": "phase.copy", "type": "progress", "name": "Copying the system", "value": 36, "max": 100 },
            { "ref": "phase.boot", "type": "progress", "name": "Setting up boot", "value": 0 },
            { "ref": "out", "type": "log", "name": "Details",
              "lines": (1..=300).map(|n| format!("line {n}")).collect::<Vec<_>>() },
        ],
    }))
    .unwrap()
}

#[test]
fn a_job_under_way_is_drawn_from_what_installerd_sends() {
    let Page::Progress {
        title,
        job,
        summary,
        disk,
        phases,
        lines,
        said,
        ended,
    } = Page::of(&installing(), None)
    else {
        panic!("not a job's page")
    };
    assert_eq!((title.as_str(), job.as_str()), ("Installing", "install"));
    assert!(summary.starts_with("Installing Peios onto CT500MX500SSD1"));
    assert_eq!(disk.unwrap().device, "/dev/sda");
    assert_eq!(
        phases.iter().map(|p| p.r#ref.as_str()).collect::<Vec<_>>(),
        [
            "phase.partition",
            "phase.format",
            "phase.copy",
            "phase.boot"
        ]
    );
    assert_eq!(
        phases[2],
        Phase {
            r#ref: "phase.copy".into(),
            name: "Copying the system".into(),
            value: 36.0,
            max: Some(100.0),
            detail: Value::Null
        }
    );
    // One that does not say how far there is to go.
    assert_eq!(phases[3].max, None);
    // A page is sent the last of what the job said, and how much there
    // was; the whole of it is kept to be served as a file.
    assert_eq!(said, 300);
    assert_eq!(lines.len(), 200);
    assert_eq!(
        (lines[0].as_str(), lines[199].as_str()),
        ("line 101", "line 300")
    );
    assert_eq!(super::said(&installing()).len(), 300);
    assert!(super::said(&mode()).is_empty());
    assert_eq!(ended, None);

    // The upgrade's and the repair's are the same page, of another job,
    // and an installerd that says it only in words is drawn without the
    // disk.
    let mut turn = installing();
    turn.id = Some("upgrade.progress".into());
    turn.elements[0].state.remove("detail");
    assert!(
        matches!(Page::of(&turn, None), Page::Progress { job, disk: None, .. } if job == "upgrade")
    );
    turn.id = Some("repair.progress".into());
    assert!(matches!(Page::of(&turn, None), Page::Progress { job, .. } if job == "repair"));

    let sent = serde_json::to_value(Page::of(&installing(), None)).unwrap();
    assert_eq!(sent["kind"], "progress");
    assert_eq!(
        sent["phases"][2],
        json!({ "ref": "phase.copy", "name": "Copying the system", "value": 36.0, "max": 100.0 })
    );
    assert_eq!(sent["ended"], Value::Null);
}

#[test]
fn a_page_not_drawn_here_is_said_to_be_one() {
    let mut turn = mode();
    turn.id = Some("upgrade.confirm".into());
    turn.name = Some("Ready to upgrade".into());
    assert_eq!(
        Page::of(&turn, None),
        Page::Unbuilt {
            id: "upgrade.confirm".into(),
            title: "Ready to upgrade".into()
        }
    );
    turn.id = None;
    assert!(matches!(Page::of(&turn, None), Page::Unbuilt { id, .. } if id.is_empty()));
}

#[test]
fn a_page_is_sent_as_the_kind_it_is() {
    let view = View {
        generation: 1,
        conversation: Conversation::Install,
        release: Release {
            version: "2026.8".into(),
            variant: "Experimental".into(),
        },
        link: Link::Connected {
            daemon: "installerd/0.1.10".into(),
        },
        trail: vec![("connect".into(), "/run/installerd.sock".into())],
        seq: 1,
        waiting: Some("act.install".into()),
        page: Some(Page::of(&mode(), None)),
        restarting: false,
        boot: "8c7c".into(),
    };
    let sent = serde_json::to_value(&view).unwrap();
    assert_eq!(sent["conversation"], "install");
    assert_eq!(serde_json::to_value(Conversation::Oobe).unwrap(), "oobe");
    assert_eq!(
        sent["link"],
        json!({ "state": "connected", "daemon": "installerd/0.1.10" })
    );
    assert_eq!(sent["seq"], 1);
    assert_eq!(sent["waiting"], "act.install");
    assert_eq!(sent["page"]["kind"], "mode");
    assert_eq!(
        sent["page"]["actions"][0],
        json!({
            "ref": "act.install", "name": "Install Peios", "primary": true, "enabled": true, "help": null,
            "destructive": false, "unbuilt": false,
        })
    );
    assert_eq!(sent["trail"], json!([["connect", "/run/installerd.sock"]]));
    assert_eq!(
        (sent["restarting"].clone(), sent["boot"].clone()),
        (json!(false), json!("8c7c"))
    );
    assert_eq!(
        sent["release"],
        json!({ "version": "2026.8", "variant": "Experimental" })
    );

    let sent = serde_json::to_value(Page::of(&disks(), Some("/dev/sda"))).unwrap();
    assert_eq!(sent["kind"], "disk");
    assert_eq!(sent["chosen"], "/dev/sda");
    assert_eq!(sent["disks"][0]["detail"]["bytes"], 500107862016u64);
    assert_eq!(sent["next"]["unbuilt"], false);
}

#[test]
fn an_ending_says_how_it_ended() {
    let end = End {
        seq: 9,
        outcome: Outcome::Failed,
        message: Some("cp failed (exit 1)".into()),
    };
    let ending = Page::Ended {
        outcome: "failed",
        message: "cp failed (exit 1)".into(),
    };
    assert_eq!(Page::ended(None, &end), ending);
    assert_eq!(Page::ended(Some(Page::of(&mode(), None)), &end), ending);
}

#[test]
fn a_job_that_has_ended_stays_on_its_page_and_says_how() {
    let end = End {
        seq: 9,
        outcome: Outcome::Failed,
        message: Some("copying /boot: cp failed (exit 1)".into()),
    };
    let Page::Progress {
        title,
        phases,
        said,
        ended,
        ..
    } = Page::ended(Some(Page::of(&installing(), None)), &end)
    else {
        panic!("the job's page gave way to the ending")
    };
    // Everything on it is as it was when the job stopped.
    assert_eq!(
        (title.as_str(), phases[2].value, said),
        ("Installing", 36.0, 300)
    );
    assert_eq!(
        ended,
        Some(Ended {
            outcome: "failed",
            message: "copying /boot: cp failed (exit 1)".into(),
            reboot: None,
            start: None,
            error: None,
        })
    );
    let done = End {
        seq: 9,
        outcome: Outcome::Complete,
        message: None,
    };
    let sent =
        serde_json::to_value(Page::ended(Some(Page::of(&installing(), None)), &done)).unwrap();
    assert_eq!(sent["kind"], "progress");
    assert_eq!(
        sent["ended"],
        json!({ "outcome": "complete", "message": "", "reboot": null, "start": null, "error": null })
    );
}

/// oobed's first page, as it sends it (`oobed/src/flow.rs`).
fn welcome() -> Turn {
    serde_json::from_value(json!({
        "seq": 1, "id": "oobe.locale", "name": "Welcome to Peios",
        "elements": [
            { "ref": "locale.intro", "type": "text", "text": "A few questions and this machine is ready to use." },
            { "ref": "locale.language", "type": "select", "name": "Language", "choices": [], "enabled": false,
              "help": "Peios ships in English only for now; there is no locale data to choose from yet." },
            { "ref": "locale.keyboard", "type": "select", "name": "Keyboard layout", "choices": [], "enabled": false,
              "help": "No keymaps are packaged yet." },
            { "ref": "nav.next", "type": "action", "name": "Next", "primary": true },
        ],
    }))
    .unwrap()
}

#[test]
fn first_boot_setup_is_welcomed_from_what_oobed_sends() {
    let Page::Welcome {
        title,
        intro,
        language,
        next,
    } = Page::of(&welcome(), None)
    else {
        panic!("not the welcome")
    };
    assert_eq!(
        (title.as_str(), intro.as_str()),
        (
            "Welcome to Peios",
            "A few questions and this machine is ready to use."
        )
    );
    let language = language.unwrap();
    assert_eq!(
        (
            language.r#ref.as_str(),
            language.name.as_str(),
            language.enabled
        ),
        ("locale.language", "Language", false)
    );
    assert!(language.choices.is_empty());
    assert!(
        language
            .help
            .unwrap()
            .starts_with("Peios ships in English only")
    );
    let next = next.unwrap();
    assert!(next.primary && next.enabled);
    // The network page it leads to is drawn.
    assert!(!next.unbuilt);
    // The keyboard is the browser's, and is not sent.
    let sent = serde_json::to_value(Page::of(&welcome(), None)).unwrap();
    assert_eq!(sent["kind"], "welcome");
    assert!(!sent.to_string().contains("keyboard"));
}

#[test]
fn a_choice_is_a_value_and_a_name() {
    let mut turn = welcome();
    turn.elements[1].state.insert(
        "choices".into(),
        json!([{ "value": "en-GB", "name": "English (United Kingdom)" }, { "value": "de" }, "fr", { "value": "nl", "name": "Nederlands" }]),
    );
    let Page::Welcome {
        language: Some(language),
        ..
    } = Page::of(&turn, None)
    else {
        panic!("no language")
    };
    assert_eq!(
        language.choices,
        [
            ("en-GB".into(), "English (United Kingdom)".into()),
            ("nl".into(), "Nederlands".into())
        ]
    );
}

/// oobed's network page, as it sends it (`oobed/src/flow.rs`), with one
/// interface connected.
fn network() -> Turn {
    serde_json::from_value(json!({
        "seq": 2, "id": "oobe.network", "name": "Network",
        "elements": [
            { "ref": "network.status", "type": "text",
              "text": "This machine is connected to a network.\neth0: connected, as 10.0.2.15/24, through 10.0.2.2.",
              "detail": { "readiness": "routed", "interfaces": [
                  { "name": "eth0", "state": "connected", "addresses": ["10.0.2.15/24"], "dns": ["10.0.2.3"], "gateway": "10.0.2.2" },
              ] } },
            { "ref": "network.note", "type": "text", "text": "Setup does not need a network, and nothing here has to be answered." },
            { "ref": "network.refresh", "type": "action", "name": "Check again", "validate": false },
            { "ref": "network.wifi", "type": "action", "name": "Connect to Wi-Fi…", "enabled": false,
              "help": "No wireless stack is packaged yet." },
            { "ref": "network.static", "type": "action", "name": "Configure manually…", "validate": false },
            { "ref": "nav.back", "type": "action", "name": "Back", "validate": false },
            { "ref": "nav.next", "type": "action", "name": "Next", "primary": true },
        ],
    }))
    .unwrap()
}

#[test]
fn the_network_is_drawn_from_what_oobed_sends() {
    let Page::Network {
        title,
        status,
        interfaces,
        note,
        planned,
        refresh,
        manual,
        unplan,
        others,
        back,
        next,
    } = Page::of(&network(), None)
    else {
        panic!("not the network page")
    };
    assert_eq!(title, "Network");
    assert!(status.starts_with("This machine is connected to a network.\n"));
    assert!(note.starts_with("Setup does not need a network"));
    let interfaces = interfaces.unwrap();
    assert_eq!(
        (interfaces.len(), &interfaces[0]["state"]),
        (1, &json!("connected"))
    );
    // Checking again is answered: it changes the page, and leads nowhere.
    assert!(!refresh.unwrap().unbuilt);
    assert!(!back.unwrap().unbuilt);
    // Addressing by hand leads to a page that is drawn.
    let manual = manual.unwrap();
    assert!(manual.enabled && !manual.unbuilt);
    assert_eq!((planned, unplan), (None, None));
    let others: Vec<_> = others
        .iter()
        .map(|a| (a.r#ref.as_str(), a.enabled, a.unbuilt))
        .collect();
    assert_eq!(others, [("network.wifi", false, true)]);
    // The account page it leads to is drawn.
    assert!(!next.unwrap().unbuilt);

    // With an address kept for the end, it says so, and it can be given up.
    let mut kept = network();
    kept.elements.insert(1, serde_json::from_value(json!({
        "ref": "network.planned", "type": "text", "text": "At the end of setup, eth0 is given 10.0.0.5/24.",
        "detail": { "interface": "eth0", "address": "10.0.0.5/24", "dns": [] },
    })).unwrap());
    kept.elements.insert(6, serde_json::from_value(json!({
        "ref": "network.unplan", "type": "action", "name": "Use the network's address instead", "validate": false,
    })).unwrap());
    let Page::Network {
        planned,
        unplan,
        others,
        ..
    } = Page::of(&kept, None)
    else {
        panic!("not the network page")
    };
    let planned = planned.unwrap();
    assert_eq!(
        (planned.words.as_str(), &planned.detail["address"]),
        (
            "At the end of setup, eth0 is given 10.0.0.5/24.",
            &json!("10.0.0.5/24")
        )
    );
    assert!(unplan.is_some_and(|a| a.enabled && !a.unbuilt));
    assert_eq!(others.len(), 1);

    // An oobed that says it only in words, or could not ask netd.
    let mut words = network();
    words.elements[0].state.remove("detail");
    let Page::Network { interfaces, .. } = Page::of(&words, None) else {
        panic!("not the network page")
    };
    assert_eq!(interfaces, None);
}

/// oobed's manual page, as it sends it, with the address turned down.
fn manual() -> Turn {
    serde_json::from_value(json!({
        "seq": 3, "id": "oobe.network.manual", "name": "Configure manually",
        "elements": [
            { "ref": "manual.intro", "type": "text", "text": "It is applied at the end of setup." },
            { "ref": "manual.interface", "type": "table", "name": "Interface", "required": true, "default": "eth0",
              "columns": [{ "key": "name", "name": "Interface" }, { "key": "state", "name": "Status" }, { "key": "address", "name": "Address now" }],
              "rows": [
                  { "value": "eth0", "cells": { "name": "eth0", "state": "connected", "address": "10.0.2.15/24" } },
                  { "value": "wlan0", "cells": { "name": "wlan0", "state": "not used", "address": "" }, "enabled": false, "note": "not wired" },
              ],
              "empty": "This machine has no network hardware that Peios can use." },
            { "ref": "manual.address", "type": "string", "name": "Address", "required": true, "placeholder": "192.168.1.20/24",
              "help": "With the length of its network after a slash.", "error": "An address and the length of its network, as 192.168.1.20/24." },
            { "ref": "manual.gateway", "type": "string", "name": "Gateway", "placeholder": "192.168.1.1" },
            { "ref": "manual.dns", "type": "string", "name": "Name servers", "default": "1.1.1.1" },
            { "ref": "nav.back", "type": "action", "name": "Back", "validate": false },
            { "ref": "manual.save", "type": "action", "name": "Save", "primary": true },
        ],
    }))
    .unwrap()
}

#[test]
fn an_address_by_hand_is_asked_for_as_oobed_asks() {
    let Page::Manual {
        title,
        intro,
        interfaces,
        empty,
        assumed,
        error,
        fields,
        back,
        save,
    } = Page::of(&manual(), None)
    else {
        panic!("not the manual page")
    };
    assert_eq!(
        (title.as_str(), intro.as_str()),
        ("Configure manually", "It is applied at the end of setup.")
    );
    let rows: Vec<_> = interfaces
        .iter()
        .map(|r| (r.value.as_str(), r.enabled, r.note.as_str()))
        .collect();
    assert_eq!(rows, [("eth0", true, ""), ("wlan0", false, "not wired")]);
    assert_eq!(interfaces[0].cells["address"], "10.0.2.15/24");
    assert!(empty.starts_with("This machine has no network hardware"));
    assert_eq!((assumed.as_deref(), error), (Some("eth0"), None));
    let fields: Vec<_> = fields
        .iter()
        .map(|f| {
            (
                f.r#ref.as_str(),
                f.required,
                f.default.as_deref(),
                f.error.is_some(),
            )
        })
        .collect();
    assert_eq!(
        fields,
        [
            ("manual.address", true, None, true),
            ("manual.gateway", false, None, false),
            ("manual.dns", false, Some("1.1.1.1"), false)
        ]
    );
    assert!(back.is_some() && save.is_some_and(|s| s.primary && !s.unbuilt));
}

#[test]
fn only_what_the_page_asks_for_is_answered() {
    let said = json!({
        "manual.interface": "eth0", "manual.address": "10.0.0.5/24",
        // Not a row that can be chosen, not text, not the page's, not asked.
        "manual.gateway": 7, "manual.intro": "x", "act.begin": "x", "disk.target": "/dev/sda",
    });
    let filled = filled(&manual(), said.as_object().unwrap().clone());
    assert_eq!(
        Value::Object(filled),
        json!({ "manual.interface": "eth0", "manual.address": "10.0.0.5/24" })
    );
    let wlan = json!({ "manual.interface": "wlan0" });
    assert!(
        super::filled(&manual(), wlan.as_object().unwrap().clone()).is_empty(),
        "a row that cannot be chosen"
    );
}

/// oobed's account page, as it sends it, with the passwords turned down.
fn account() -> Turn {
    serde_json::from_value(json!({
        "seq": 4, "id": "oobe.account", "name": "Create your account",
        "elements": [
            { "ref": "account.intro", "type": "text", "text": "This account administers the machine." },
            { "ref": "account.name", "type": "string", "name": "User name", "required": true, "default": "peios",
              "help": "What you sign in as." },
            // A secret oobed has started with something, which it never does.
            { "ref": "account.password", "type": "string", "name": "Password", "required": true, "secret": true, "default": "hunter2" },
            { "ref": "account.confirm", "type": "string", "name": "Confirm password", "required": true, "secret": true,
              "error": "The passwords do not match." },
            { "ref": "nav.back", "type": "action", "name": "Back", "validate": false },
            { "ref": "nav.next", "type": "action", "name": "Next", "primary": true },
        ],
    }))
    .unwrap()
}

#[test]
fn the_account_is_asked_for_as_oobed_asks() {
    let Page::Account {
        title,
        intro,
        fields,
        back,
        next,
    } = Page::of(&account(), None)
    else {
        panic!("not the account page")
    };
    assert_eq!(
        (title.as_str(), intro.as_str()),
        (
            "Create your account",
            "This account administers the machine."
        )
    );
    let fields: Vec<_> = fields
        .iter()
        .map(|f| {
            (
                f.r#ref.as_str(),
                f.secret,
                f.default.as_deref(),
                f.error.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        fields,
        [
            ("account.name", false, Some("peios"), None),
            ("account.password", true, None, None),
            (
                "account.confirm",
                true,
                None,
                Some("The passwords do not match.")
            ),
        ]
    );
    // Nothing of a password is sent to everyone looking.
    assert!(
        !serde_json::to_value(Page::of(&account(), None))
            .unwrap()
            .to_string()
            .contains("hunter2")
    );
    assert!(back.is_some_and(|b| !b.unbuilt));
    // The naming page it leads to is drawn.
    assert!(next.is_some_and(|n| n.primary && !n.unbuilt));
    // What is typed is answered with, passwords and all.
    let said = json!({ "account.name": "jack", "account.password": "one", "account.confirm": "one", "account.intro": "x" });
    let filled = filled(&account(), said.as_object().unwrap().clone());
    assert_eq!(
        Value::Object(filled),
        json!({ "account.name": "jack", "account.password": "one", "account.confirm": "one" })
    );
}

/// oobed's naming page, as it sends it.
fn naming() -> Turn {
    serde_json::from_value(json!({
        "seq": 5, "id": "oobe.naming", "name": "Name this machine",
        "elements": [
            { "ref": "hostname", "type": "string", "name": "Machine name", "required": true, "default": "peios-3f2a",
              "help": "How this machine identifies itself on a network.", "min": 1, "max": 63, "pattern": "[A-Za-z0-9]([A-Za-z0-9-]*[A-Za-z0-9])?" },
            { "ref": "domain.join", "type": "action", "name": "Join a domain instead…", "enabled": false,
              "help": "Domain membership needs a directory source, which is not built yet." },
            { "ref": "nav.back", "type": "action", "name": "Back", "validate": false },
            { "ref": "nav.finish", "type": "action", "name": "Finish", "primary": true },
        ],
    }))
    .unwrap()
}

#[test]
fn the_name_is_asked_for_as_oobed_asks() {
    let Page::Naming {
        title,
        name,
        others,
        back,
        finish,
        account: whose,
    } = Page::of(&naming(), None)
    else {
        panic!("not the naming page")
    };
    // oobed does not say whose account it is; this process says so once
    // it has heard it answered.
    assert_eq!(whose, None);
    let Page::Naming { account: whose, .. } = Page::of(&naming(), None).for_account(Some("jack"))
    else {
        panic!("not the naming page")
    };
    assert_eq!(whose.as_deref(), Some("jack"));
    assert_eq!(
        Page::of(&account(), None).for_account(Some("jack")),
        Page::of(&account(), None)
    );
    assert_eq!(title, "Name this machine");
    let name = name.unwrap();
    assert_eq!(
        (name.r#ref.as_str(), name.default.as_deref(), name.max),
        ("hostname", Some("peios-3f2a"), Some(63))
    );
    assert_eq!(
        name.pattern.as_deref(),
        Some("[A-Za-z0-9]([A-Za-z0-9-]*[A-Za-z0-9])?")
    );
    let others: Vec<_> = others
        .iter()
        .map(|a| (a.r#ref.as_str(), a.enabled, a.help.is_some()))
        .collect();
    assert_eq!(others, [("domain.join", false, true)]);
    assert!(back.is_some_and(|b| !b.unbuilt));
    // Applying is drawn.
    assert!(finish.is_some_and(|f| f.primary && !f.unbuilt));
    let said = json!({ "hostname": "workshop", "domain.join": "x" });
    assert_eq!(
        Value::Object(filled(&naming(), said.as_object().unwrap().clone())),
        json!({ "hostname": "workshop" })
    );
}

/// oobed's applying page, as it sends it, with an address given by hand
/// and the account made.
fn applying() -> Turn {
    serde_json::from_value(json!({
        "seq": 6, "id": "oobe.applying", "name": "Finishing setup", "class": ["progress"],
        "elements": [
            { "ref": "phase.account", "type": "progress", "name": "Creating your account", "value": 100, "max": 100 },
            { "ref": "phase.hostname", "type": "progress", "name": "Naming the machine", "value": 0, "max": 100 },
            { "ref": "phase.network", "type": "progress", "name": "Addressing eth0", "value": 0, "max": 100,
              "detail": { "interface": "eth0", "address": "10.0.2.20/24", "gateway": "10.0.2.2", "dns": ["10.0.2.3"] } },
            { "ref": "out", "type": "log", "name": "Details", "lines": ["creating jack as an administrator"] },
        ],
    }))
    .unwrap()
}

#[test]
fn setup_applying_is_a_job_like_the_installer_s() {
    let Page::Progress {
        title,
        job,
        disk,
        phases,
        lines,
        said,
        ended,
        ..
    } = Page::of(&applying(), None)
    else {
        panic!("not a job's page")
    };
    assert_eq!(
        (title.as_str(), job.as_str(), disk, said, ended),
        ("Finishing setup", "setup", None, 1, None)
    );
    assert_eq!(lines, ["creating jack as an administrator"]);
    let phases: Vec<_> = phases.iter().map(|p| (p.r#ref.as_str(), p.value)).collect();
    assert_eq!(
        phases,
        [
            ("phase.account", 100.0),
            ("phase.hostname", 0.0),
            ("phase.network", 0.0)
        ]
    );
    // Where the machine is going is passed on with the phase that takes
    // it there, and nothing is said of the others.
    let sent = serde_json::to_value(Page::of(&applying(), None)).unwrap();
    assert_eq!(sent["phases"][2]["detail"]["address"], "10.0.2.20/24");
    assert!(sent["phases"][0].get("detail").is_none());
    // Ending, it stays, and says how it ended.
    let end: End = serde_json::from_value(json!({ "seq": 6, "outcome": "complete", "message": "Setup is complete. You can log in now." })).unwrap();
    let Page::Progress { ended, phases, .. } = Page::ended(Some(Page::of(&applying(), None)), &end)
    else {
        panic!("the job's page gave way")
    };
    assert_eq!((ended.unwrap().outcome, phases.len()), ("complete", 3));
}

/// installerd's page after an install that finished
/// (`installerd/src/flow.rs`, `done_page`).
fn finished() -> Turn {
    serde_json::from_value(json!({
        "seq": 5, "id": "install.done", "name": "Installation complete", "class": ["done"],
        "elements": [
            { "ref": "done.summary", "type": "text", "text": "Installation complete. Reboot to start Peios." },
            { "ref": "act.reboot", "type": "action", "name": "Reboot now", "primary": true },
            { "ref": "nav.start", "type": "action", "name": "Back to the start", "validate": false },
        ],
    }))
    .unwrap()
}

#[test]
fn a_job_that_finished_stays_on_its_page_with_the_way_on() {
    assert!(is_done(&finished()));
    assert!(!is_done(&installing()));
    let Page::Progress {
        title,
        phases,
        said,
        ended,
        ..
    } = Page::done(Some(Page::of(&installing(), None)), &finished())
    else {
        panic!("the job's page gave way")
    };
    // Everything on it is as the job left it.
    assert_eq!((title.as_str(), phases.len(), said), ("Installing", 4, 300));
    let ended = ended.unwrap();
    assert_eq!(
        (ended.outcome, ended.message.as_str()),
        ("complete", "Installation complete. Reboot to start Peios.")
    );
    let reboot = ended.reboot.unwrap();
    assert_eq!(
        (reboot.r#ref.as_str(), reboot.name.as_str(), reboot.primary),
        ("act.reboot", "Reboot now", true)
    );
    assert_eq!(ended.start.unwrap().r#ref, "nav.start");
    assert_eq!(ended.error, None);

    // A restart installerd could not make is said with it.
    let mut refused = finished();
    refused.elements[1].error = Some("peinit did not take the request".into());
    let again = Page::done(
        Some(Page::done(Some(Page::of(&installing(), None)), &finished())),
        &refused,
    );
    let Page::Progress { said, ended, .. } = again else {
        panic!("not the job's page")
    };
    assert_eq!(said, 300);
    assert_eq!(
        ended.unwrap().error.as_deref(),
        Some("peinit did not take the request")
    );

    // Joined on this page, with nothing seen before it.
    let Page::Progress {
        title,
        job,
        phases,
        ended,
        ..
    } = Page::done(None, &finished())
    else {
        panic!("not the job's page")
    };
    assert_eq!(
        (title.as_str(), job.as_str(), phases.len()),
        ("Installation complete", "install", 0)
    );
    assert!(ended.unwrap().reboot.is_some());
}
