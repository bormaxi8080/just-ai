# Validated modularization plans

Modularization is an application use case. A deterministic plan preserves recipe
comments and attributes and shows every proposed file. It refuses existing
module destinations and existing imported/module projects rather than silently
moving recipes across source boundaries. Validation happens in an isolated
staging directory before any destination is changed.

New module files are persisted without clobbering; the root justfile rename is
the commit point. Returned errors roll back newly created modules. All companion
patches acquire a create-new writer lock and recheck reviewed content immediately
before committing. Locks coordinate companion writers, not arbitrary editors.
A multi-file update is not crash-atomic; interruption before the root commit may
leave new unreferenced modules and a stale lock. Inspect these before removing
the lock and retrying. Never delete an existing module automatically.

Deduplication also uses a reviewed application plan. It removes only exact
source duplicates after normalizing the recipe name, retains referenced recipes,
and refuses existing imported/module projects. Parameter order, interpolations
and repeated commands are preserved. Non-equivalent semantic merges are rejected.
