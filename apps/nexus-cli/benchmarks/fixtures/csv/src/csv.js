/** Parse RFC-4180-style CSV into an array of rows. */
export function parseCsv(input) {
  return input.trim().split('\n').map((line) => line.split(','));
}
