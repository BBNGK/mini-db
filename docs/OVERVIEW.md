# Overview
### Network Layer
- Handles communication between the client (e.g. application using the DB) and the DB itself 
### Front-End
- Responsible for converting query into something backend can process (`query processor`)
- Handles tokenizing / parsing / optimizing query for execution engine
- Passes data to execution engine
### Execution Engine
- Ensures query is executed properly (`query executor`)
- Handles caching (`cache manager`) and utility services (e.g. auth, backups, metrics, etc.)
### "Transaction Layer"
- `ACID` layer (Atomicity, Consistency, Isolation, Durability)
	- Handles transaction states and management (`transaction manager`)
	- Handles concurrent queries and resource management (`lock manager` and `concurrency manager`)
		- MVCC
	- Manages Write-Ahead Logs (`WAL`) and handles crash recovery (`recovery manager`)
### Storage Engine
- Takes care of releasing (`disk storage manager`), processing (`buffer manager`), and keeping retrieval speeds fact (`index manager`)
### OS Interaction Layer
- Handles cross-platform OS calls
