Note:

This was actually pretty competitive I first got it running. Especially on the longer path opportunities (3+ hops).

But it's since fallen behind as I've left it to stagnate due to the explosion of the number of markets (it's a lot of requests to make and paths to optimize over) and to the dominance of new DEXes that have popped up. Cetus remains #1 but DeepBook, Aftermath, and FlowX have come ahead of KriyaDex and Turbos.

I think if you get an endpoint with a high rate limit (you'll have to pay or set up your own) and a fairly beefy computer to run this on, it could be competitive again on the currently integrated DEXes given a couple tweaks: limiting the list of markets to those with higher volume, not subscribing to markets with high event emission rates, and generally doing what we can to get the highest return on the amount of compute time we put in.

> A lot of this can just be done via hardcoding a couple lists of addresses. I always meant to write a script to grab market address sorted by 24 hr volume but ever got around to it.

> Even now, as it runs extremely slowly, it's able to grab a couple cents here and there!

### Ok so a little rundown is due.

Directory:
- `arb-bot`
    - This contains all of the bot logic and contains the path finding logic, the optimization logic, the order construction logic, and the order execution logic.
- `custom-sui-sdk`
    - This contains a mix of modified and new doohickies I built for interacting with the Sui chain and its contracts. This is not project specific, but it enables me to do things I wasn't able to do with the currently available Rust SDK. These doohickies are essential for writing the contract bindings I need.
- `librarian`
    - Our implementation of a "level ii" orderbook implementation of our client side representations of on-chain markets.

> Admittedly I could've done a lot better in terms of organizing the directory structure but my priority when building this was to get something that could actually make money, even if it was not very good at it.

### arb-bot

The original plan was for this to be an event-driven backrunning bot. A backrunning bot is the most efficient type of arbitrage bot there is as it "backruns" or follows up a trade that bumps the price on one exchange out-of-line with another and immediately closes the opportunity, ideally before other bots. Such a bot is far more efficient and faster to respond than a bot that assesses all markets for arbitrage opportunities every `x` seconds.

The current bot is set up to be that way. It responds to trade events coming off of a stream. However, we have a couple bottlenecks.

**Fetching is SLOW**

To preface there is no way to freely and quickly query the chain for the outcome of executing a specific function on a specific contract. In the context of out arbitrage bot, once we've judged that there is an opportunity, we would do best to optimize the amount we intially trade at the start of arbitrage path to maximize our profit. To do this we would have to compute how much we get out given how much we trade in for every leg of the trade. And we would have to do that client side.

However, these exchanges aren't built off the standard orderbook model. The programs underlying these exchanges range in the complexity of how people can provide liquidity on them and how that liquidity is processed when people trade on these exchanges. Depending on the complexity of the logic, the state size can range largely. Among the exchanges I have chosen to incorporate, are a few that have a pretty large state size. Pulling that state from the chain typically requires a couple requests; the Sui endpoints can only provide so much data per request and the free endpoint I'm using limits me to about 40 requests per second. I've seen it take 3-4 requests on the low end and 10+ requests to pull a market's info from the chain. With the number of maximum intermediate markets in a possible arbitrage path set to 3, I've seen the number of different markets in all the possible cycles containing our source coin and the market in which a trade event occure be 100+. This makes the step of pulling information extremely slow. Even limited to 1 intermediate market, it can take several seconds to fetch all the considered markets' states. Assuming that someone has figured out a way to fetch information faster - even just doing what I am doing but with a higher number of allowed requests per second - they'll likely get to the trade first and make the information I trade on stale and myself poorer.

Now, I did consider borrowing the basic principles of a staple of traditional finance: the client-side level II orderbook. Basically we start by spinning up a stream of all the deltas accompanied by sequence numbers to apply to the orderbook and cache them. As we're caching we fetch a full snapshot of the orderbook accomapnied with a sequence number and start applying all cached deltas that follow the snapshots sequence number until we've played catch up with the live orderbook.

The idea for these more complex exchanges was to take a snapshot of the full state of a marker and apply all deltas we got via subscribing to the market's event stream. The market's emitted events describe the state-changing function called and the provided arguments. These deltas however are a significantly less uniform than those of a standard orderbook due to all the possible functions and arguments that could change state. And these differ per exchange due to differences in trading function and even implementation.

> I did end up writing all the support for applying deltas but could never test in in prod due to lack of support. I even wrote code that would utilize a currently unaivailable request deep in the codebase that the endpoints don't currently support but have all the capacity too. This request would give us an atomic snapshot with a reference point we could work with. But the reference point is not a timestamp. It's a checkpoint and the workaournd to make it work like a timestamp and place the events and the snapshot in time would be an enormous pain to work with, but I thought it through.

> The Uniswap V3 codebase should give you an idea of all the possible ways to update state in our most complex exchanges. Our most complex exchanges are implementations of Uniswap V3. It was important to implement each of the exchanges implementations in Rust to a capital T as the nature of the trading function makes precision very important. Also the way state is store differs enough that it also influenced my decision to do the above. I'll get into the Rust implementations of our exchanges trading functions later - that's a whole difference story.

> However, my overcommitment to this did eat up a good chunk of time. Since none of the exchanges code was open source, I read the assembly for each one and hand decompiled them (there was no avilable decompiler for Sui Move at the time) and wrote Rust implementations (they were originally written in Sui's particular version of the ssmart contract language Move). The two that were implementations of Uni V3 turned out to be very slow and not to different. Having been told by someone who does this stuff, onchain arbitrage and whatnoy, professionally how fast their implementation of the Uni V3 type pools on a different chain was, I knew I needed to come up with a faster implementation. I ended taking my learnings and understandings of the Uni V3 implementations and collapsing them into one adaptable and much faster (than both) implementation.

However, the current endpoints have limitations in terms of what they can provide. Since the state of markets is so large, they require multiple requests each to get the full state. Even though the endpoint offers paging, it does *not* guarantee that the information provided in previous pages hasn't changed. So we can't guarantee an atomic snapshot.

> This quirk gave me one of my hardest to discover bugs. My client side market implementations of the Uni-V3 based exchanges would occasionally break in production but I couldn't seem to find the reason. (The following is going to be hard to follow if you don't know the specifics of Uni-V3). Basically there's a global liquidity tracking variable and liquidity variables for every tick where liquidity is available. When running tick transitions, during which we add and subtract the ticks's liquidity to the global liquidity for every tick we cross into and out of, we should never run into the case of negative global liquidity. But we kept running into that case - we kept getting an under flow error. Suspecting that maybe we were missing ticks, I tried "crossing" all the ticks start to finish which should have resulting in a final liquidity figure of 0. I realized that occasionally this was the case. So I threw out all markets where this happened. Then I realized I was still getting this error. Hmm, was there information retrieved at different times that might not agree? If all the ticks agree what might not. Then I realized that due to the nature of how information is retreived from the chain, my global variables were fetched ahead of all the ticks. I ran a tick transition up to the current tick, which corresponds to the global liquidity, and a tick transition after to see if the liqudity figures agreed. They didn't. I added check to exclude any markets for which the liquidity figures don't agree and have not experienced a problem since.

**Calculating/Optimizing the Amount to Trade is SLOW**

While I initially thought that path finding would be a huge bottleneck, I came to realized that I could just precompute cycles up to a specified length.

Then came the costly part of generating the inputs for our trades. Calculating/optimizing the amount to trade to yield the maximum profit or a profit at all. I initially thought that this was going to be a very speedy process, having seen some closed form solutions for this in the past. But that solution is for one specific market trading function and it only applies when all markets along the aribtrage path are of the same type. Here we have 3 different trading functions. Thankfully all these trading functions fall into a category of trading function that makes it so that the problem of solving optimal arbitrage is possible with a sinle variable (initial amount in) numerical method.

I opted for golden section search. Basically we chain the trading functions of every market on a candidate path (selected by calculating disrepancies using only price - efficient) using the outputs of the previous market trading function and the input for the next. And then we optimize the intial amount in. There are a couple quirks to these numerical methods like the tendency to get stuck in local minima/maxima due to the nature of integer math and the more continuous nature of the daisy chained trading function. But I found some workarounds by restricting the set of inputs I was allowed.

However, doing this over 20, 30, 40+ candidate paths of variable lengths takes a while.

I'm sure I could optimize my implementations of the trading functions further and the optmization function itself but it's not the biggest bottleneck.

> This was actually a much bigger pain than I'm making it sound.

**Execution is a PAIN**

Another funky quirk of the chain is that although the exchange contracts on-chain could support executing all legs of the arb atomically (as is standard on other chains), they are currently written so that they can't. Mostly because the logic would be very complex and a humongous pain to deal with due to the chain's storage model, which I have humongous gripes with and a total hate-hate relationship.

But due to things that require familiarity with Sui's storage model and contracts to understand, all arbs must be non-atomic. In fact, to use the output of a swap (as all the exchanges have implemented it), the tokens we got out for swapping some other tokens in, we have to query the chain for the ID(s) our newly gained token objects and use it/them as an input for the next swap. So we have to call the function and wait for a response which decribe the changes to storage the function caused, parse those changes out, and repeat until the whole arb is done.

> This might be the biggest bottleneck there is, but as long as we react first there should be no problem since everyone lacks this support. But reacting first is hard due to other bottlenecks regarding fetching data.

ANOTHER funky quirk is we do have to chain functions (in general function chaining is uspported) to execute the swap in the first place. Some of the inputs required to execute the swaps in the first place can *only* be fetched via an on-chain function call. However, the Rust Sui SDK lacks what the TypeScript one has: support for chaining function calls as we construct them. It has the same rudimentary building blocks the TypeScript SDK uses, but it isn't available via the SDK at all, nor is it usable. So I had to add that. I even has to mess with the building blocks of those building blocks so that we could work with tokens in the ways that we needed them - the current SDK does NOT play nicely with the storage model especially regarding tokens during their use in function calls.

ANOTHER funky quirk is that the building blocks for the current SDK fetch specific contract data to construct function calls EVERY time we decide to construct a function call for a given contract. So it's a bajillion requests to build the function calls in the first place.

> I did consider trying a solution where we cache the contract data we retrieve to reduce the number of requests but it requires getting low-level AGAIN and I had deemed it not worth it due to the lack of support provided by the nodes themselves for the solutions to my other problems I had come up with.

**Other Pains**
There are a bajillion other issues that make Sui an absolute nightmare to work with as a bot operator, but they aren't the biggest bottlenecks ATM.

**Conslusion**
I've left the main loop cobbled together cause it works and things aren't gonna get much better till the nodes offer support for the solutions I want to implement. And I can't really rewrite the node code. I could contribute but Mysten has well paid employees who have been working on it last I asked. The atomic snapshots should be in testing starting January and will be available with RPC 2.0 in late March. Let's see if I'm unemployed then and willing to work on this then. AHHAA.

Directory:
- `main.rs`
    - Grabs needed inputs from command line.
    - We fetch all the markets from the given exchanges. These exchanges are hardcoded as we have exchanges specific APIs.
    - We instantiate the market graph we will explore arbitrage paths over.
- `lib.rs`
    - Runs the loop that processes trade events coming off a stream from the given exchanges.
    - Due to bottlenecks it's slow to trade in response to an event and just forwards to the last event in the stream to process once it finishes processing one trade.
    - Actually a big pain point here too is that some exchanges emit events at the per market level whil other emit them at an exchange level. So our parsing logic differs per exchange. There was a bit of complication due to my use of traits but I found a workaround.
- `cetus_pool.rs`
    - A Uni-V3 implementation. It was slow. It's been replaced with `fast_v3_pool`.
- `turbos.rs`
    - A Uni-V3 implementation. It was slow. It's been replaced with `fast_v3_pool`.
- `kriyadex.rs`
    - A combination Uni-V2 and Solidly Stableswap implementation. Used for calculation purposes.
- `fast_v3_pool.rs`
    - My version of a Uni-V# implementation. Used for calculation purposes.
- `cetus.rs`, `turbos.rs`, `kriyadex.rs`
    - Provides an API for interactinf with the Cetus markets.
    - Supports calculation (`cetus_pool` underlies it).
    - Supports fetching information from the chain and building our client side representations of on-chain market.
    - The versions of these that support applying deltas to our client-side representations of on-chain markets exists in the `librarian` folder.
    - Contains the bindings I wrote to construct function calls.
- `market_graph.rs`
    - Supports building the grpah of markets given the markets.
    - Support finding cycles up to a given length.
- `markets.rs`
    - `Market` is a trait.
    - We've chosen to go with using traits over enums for polymorphism.
    - This way we can interact with all the markets in a uniform wa ydespite their differing underlying representations.

Overall, I felt like this was a huge waste of time (who decompiles assembly by hand?) but not by much fault of my own. I thought this was gonna be my seminal project like "woohoo" interviewers look at me, but my god was it not a good return on time (at least in terms of dineros). But looking back on this - I cannot believe it's been a whole 5 months since I put this down - I'm kind of impressed by my persistence. Y'all have no idea how much time I spent in the Sui codebase because of the lack of documentation there was regarding what I wanted to do. I really figured this out and got something working by the sweat of my brow or something like that. Like it's made a couple hundred bucks in it's lifetime! That's crazy!

I'm trying to make this short and sweet so I'm gonna toss all the EXTREMLY messy notes I wrote in some other file here. Also I don't think I have all the notes so uh yeah.