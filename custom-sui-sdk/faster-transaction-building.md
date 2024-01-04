+++
title = "faster transaction building"
date = 23-08-28
+++

### Faster faster
Out execution is painfully slow since it takes an unreasonable number of json rpc calls to build our transactions.
One obvious place in our `programmable_move_call()` function is the request it has to make for the package object (`SuiObjectData`) every time we call it.

We can cache this object easily and make it a field of the structs representing the exchanges.

There is one other async call in `programmable_move_call()`. this is `<TransactionBuilder>.get_object_arg()`, which makes a request for the object with `<ReadApi>.get_object_with_options()`. 

`get_object_arg()` also 