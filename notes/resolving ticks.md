The number of ticks in the tick map and the number of ticks (struct) are the same.
So it must be some sort of issue with resolving ticks.

OKOK SO THE PROBLEM IS POSITION TICK!!!!!!!
Because we've confirmed that the sets of tick indexes generated from the tick map and the ticks are identical.

But when `println!` out the `word_pos` and `bit_pos` for the same tick index we get different answers.
The `word_pos`'s seem one to one but the `bit_pos`'s DEFINITELY aren't.

OK the issue was with using plain % over `mod_euclidean`.

