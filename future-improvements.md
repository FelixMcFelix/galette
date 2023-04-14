# (Potential) future improvements

While evaluating this work and writing up the paper, several improvements have come to mind that may be worth investigating.

* Reimplement how tailcalls are handled and set up to better support atomicity. Single large shared prog map, use array of arrays to store actions map (store jump as, e.g., u8:tailcall_cmd + u24:prog_map_idx). Track referees of each program in userland/ctl plane to know when to remove.

* Delegate loadbalancing over cores to thread 0 in userland using a SINGLE XSK. Use of several XSKs appears to have massive kern->user sync overhead specifically in the socket read---this suggests to me that the optimal design is to have just a single XSK per umem, if possible.
