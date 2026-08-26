// The platform package for the machine this runs on, or nothing.
//
// One line of output so a shell script can read the table in src/target.ts without
// reimplementing it: publish-npm.sh uses it to find the single staged binary it is
// able to execute, and asks that one what version it thinks it is.

import { resolveTarget } from '../src/target.ts';

const target = resolveTarget();
if (target !== null) process.stdout.write(`${target.pkg} ${target.exe}\n`);
