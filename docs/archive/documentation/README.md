# Archived Documentation

These files preserve the documentation used during the installer repository
consolidation. They are historical records, outside the published Learn tree.
Commands and dependency names in them are not maintained instructions.

`sources.json` records the source pages consulted in `learn-staging`, with
their original paths, dispositions, and SHA-256 digests. The maintained
replacement is `docs/learn/1--peios.shelf/peios.product/700--installation-and-setup.shelf`.
The installation, first-boot, service, and rehearsal guidance was rewritten
against the source in this repository rather than copied as a release claim.

The mixed Installing to Disk source page remains in staging because it
also owns historical `peios-install` guidance and disk-boot mechanics from
other repositories. Its installer-specific guidance is covered by the
maintained replacement.

The PC installation guide remains in `learn-staging`: image acquisition,
firmware, medium preparation, and certificate policy cross component
boundaries. The MSIP specification and generic presentation-hint guidance
remain there for the same reason. Package installation documentation belongs
to the package manager, not this installer.
