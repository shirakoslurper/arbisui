**This is what made execution possible.**

**The Problem**

Those who want to make multiple function calls atomically (within a single transaction) on Sui will eventually come across what the Sui devs call [Programmable Transaction Blocks](https://docs.sui.io/concepts/transactions/prog-txn-blocks). 

They'll probably find them by way of [this guide](https://docs.sui.io/guides/developer/sui-101/building-ptb) to building programmable transaction blocks.

The examples in this guide are in TypeScript. Only in TypeScript. This is because the Rust SDK's transaction builder does not offer the full functionality of the TypeScript SDK's transaction builder. Importantly, it lacks the functionality that matter most to us: being able to use the outputs of function calls and inputs for other function calls.

Why is it important?

Some of the swap calls *require* calling other onchain functions to grab the objects they need as function arguments. Like grabbing a reference to the onchain clock. There's no other way to grab a a reference without an on-chain call, and it can only be used in the same transaction. We can grab a reference in one transaction and use it in another.

> This is due to the particulars of Sui Move.

Thankfully, while the Rust SDK's transaction builder does not offer the same capabilities of the TypeScript SDK's transaction builder, it's basically a wrapper around a (non-developer-friendly) programmable transaction builder that does. And to access those capabilities we only (haha) have to modify the SDK's transaction builder to expose those capabilities.

> This programmable transaction builder can be found in the `sui_types::programmable_transaction_builder` crate (in the Sui repo). 

**A little background on `ProgrammableTransactionBuilder`**

`ProgrammableTransactionBuilder` consists of sequences of inputs and commands we append to as we build our transaction:

```
#[derive(Default)]
pub struct ProgrammableTransactionBuilder {
    inputs: IndexMap<BuilderArg, CallArg>,
    commands: Vec<Command>,
}
```

Wait, dude. You said there would be a sequence of inputs. Why am I looking at a map?

Good question. The map confused me at first, too. But it turns out the order of the key-value pairs in an `IndexMap` is dependent on insertion order not the hash values of the keys. So the map *is* an effective means of storing a sequence!

This is especially important in that it means every input in `inputs` has an index along with every command in `commands`. Remember this!

> The reason `inputs` is an `IndexMap` mapping `BuilderArg`s to `CallArg`s is that we need to be able to validate the proper use of objects that are used multiple times as arguments. We refer to the map to check if we've used an object as an argument before. This is language (Sui Move) specific stuff. We don't need to be too concerned with this stuff.

Now, we're gonna go somewhere else for a bit.

This is `ProgrammableMoveCall`:
```
pub fn programmable_move_call(
    &mut self,
    package: ObjectID,
    module: Identifier,
    function: Identifier,
    type_arguments: Vec<TypeTag>,
    arguments: Vec<Argument>,
) -> Argument {
    self.command(Command::MoveCall(Box::new(ProgrammableMoveCall {
        package,
        module,
        function,
        type_arguments,
        arguments,
    })))
}
```

Notice how it returns an `Argument`? Notice the type of `arguments` is `Vec<Argument>`?

Does this ring a bell? Well, if it does it's because this is the function that enables us to do what's most important to us: using the outputs of function calls as the inputs for other function calls.

Wonder how this works?

Well take a look at the functions used to add to our sequence of inputs:

```
pub fn pure_bytes(&mut self, bytes: Vec<u8>, force_separate: bool) -> Argument {
    ...
    let (i, _) = self.inputs.insert_full(arg, CallArg::Pure(bytes));
    Argument::Input(i as u16)
}
pub fn obj(&mut self, obj_arg: ObjectArg) -> anyhow::Result<Argument> {
    ...
    let (i, _) = self
        .inputs
        .insert_full(BuilderArg::Object(id), CallArg::Object(obj_arg));
    Ok(Argument::Input(i as u16))
}
```
Notice how we get an `Argument` enum that wraps the index of the input in `inputs`?

And the function used to add a command:

```
pub fn command(&mut self, command: Command) -> Argument {
    let i = self.commands.len();
    self.commands.push(command);
    Argument::Result(i as u16)
}
```

Notice how we get an `Argument` enum that wraps the index of the command in `commands`?

Ahhhh. I see you putting 2 and 2 together. 

The end programmable transaction refers to the indices of the commands to use the results of them as arguments for other calls. The important thing to take away here is that we need an `Argument::Result(u16)` to know which of the commands to use the result of as an input for another call.

**The why behind the problem**
Knowing this, you might something peculiar when browsing through `sui_types::programmable_transaction_builder` and `sui_sdk::transaction_builder`.

The SDK's transaction builder has a function `single_move_call()` that takes a mutable reference to a `ProgrammableTransactionBuilder`. This is the function that allows use to make string together multiple calls.

```
pub async fn single_move_call(
    &self,
    builder: &mut ProgrammableTransactionBuilder,
    package: ObjectID,
    module: &str,
    function: &str,
    type_args: Vec<SuiTypeTag>,
    call_args: Vec<SuiJsonValue>,
) -> anyhow::Result<()> {
    let module = Identifier::from_str(module)?;
    let function = Identifier::from_str(function)?;

    let type_args = type_args
        .into_iter()
        .map(|ty| ty.try_into())
        .collect::<Result<Vec<_>, _>>()?;

    let call_args = self
        .resolve_and_checks_json_args(
            builder, package, &module, &function, &type_args, call_args,
        )
        .await?;

    builder.command(Command::move_call(
        package, module, function, type_args, call_args,
    ));
    Ok(())
}
```

Wait a second. The return value of `builder.command()`, which is `Argument::Result(u16)` as we saw above, isn't used anywhere? Well, this is the closest we get to being able to add an aribtrary Move call as a command and getting a usable value from it.

So I set about adding a function that would do about the same as the above function but be able to take and return `Argument`s.

This wasn't too hard. I wanted to be able to mix standard inputs with results of other functions, so I used an enum, `ProgrammableTransactionArg`, for that. As for return value I just returned `Argument` (could be wrapped by `ProgrammableTransactionArg` for use later).

```
pub async fn programmable_move_call(
        &self,
        builder: &mut ProgrammableTransactionBuilder,
        package: ObjectID,
        module: &str,
        function: &str,
        type_args: Vec<SuiTypeTag>,
        call_args: Vec<ProgrammableTransactionArg>,
    ) -> anyhow::Result<Argument> {
        let module = Identifier::from_str(module)?;
        let function = Identifier::from_str(function)?;

        let type_args = type_args
            .into_iter()
            .map(|ty| ty.try_into())
            .collect::<Result<Vec<_>, _>>()?;

        let call_args = self
            .resolve_and_checks_programmable_transaction_args(
                builder, package, &module, &function, &type_args, call_args,
            )
            .await?;

        Ok(
            builder.command(Command::move_call(
                package, module, function, type_args, call_args,
            ))
        )
    }
```

Eventually I realized that I needed a few extra programmable basic functions:
- `programmable_merge_coins()`: A "programmable" (in that I can use the output as an input) version of the SDK's `merge_coins` so I could merge coin objects of the same type and use them in the swap functions that only take a single coin object for a single coin type all in the same transaction.
- `programmable_make_object_vec()`: Exposes `programmable_transaction_builder`'s `make_object_vec()` function so that I can make a vector object of coins (these only last the lifetime of a transaction) and use it as an input for a swap function that called for a vector of coin objects.
- `programmable_split_gas_coin()`: Splitting off the gas coin (result of merging all your Sui) is the easiest way of acquiring the amount you need for a transaction. Otherwise you have to merge, split, and estimate gas all by yourself (not fun and result in a lot of failed transactions typically). Sadly the Rust SDK didn't provide this function.

These were a bit finicky but their details matter less since the same principles apply.

> Note to self: Clean up this first draft explanation lmao.