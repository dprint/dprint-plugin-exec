// appends a single byte to the file path given as the first argument.
// used by tests to count how many times a setup command runs.
import { appendFileSync } from "node:fs";

appendFileSync(Deno.args[0], "x");
