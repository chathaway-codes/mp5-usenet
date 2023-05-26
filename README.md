# mp5-usenet

A usenet implementation split across a few microservices for learning.

## Microservices

gateway maintains the TCP connections and processes user requests, routing requests to the appropriate system.

fileserve is more-or-less a key-value storage system that retrieves articles given either an article id or number.

groupserve maintains a list of groups, and indexes messages by those groups.

## What handles each command?

ARTICLE -- fileserve
BODY -- fileserve
GROUP -- stateserve (alters connection state)
HEAD -- fileserve
HELP -- gateway
IHAVE -- ??
LAST -- stateserve
LIST -- groupserve
NEWGROUPS -- groupserve
NEWNEWS -- groupserve
NEXT -- stateserve
POST -- ??
QUIT -- gateway
SLAVE -- ??
STAT -- stateserve


## References

https://www.w3.org/Protocols/rfc977/rfc977

https://datatracker.ietf.org/doc/html/rfc3977

