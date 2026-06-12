import chalk from 'chalk';
import Table from 'cli-table3';

/**
 * Print a table with headers and rows to stdout.
 */
export function printTable(headers: string[], rows: string[][]): void {
  const table = new Table({
    head: headers.map((h) => chalk.bold(h)),
    style: { head: [], border: [] },
  });
  table.push(...rows);
  console.log(table.toString());
}

/**
 * Print a success message (green checkmark).
 */
export function printSuccess(msg: string): void {
  console.log(chalk.green(`✓ ${msg}`));
}

/**
 * Print an error message (red X) to stderr.
 */
export function printError(msg: string): void {
  console.error(chalk.red(`✗ ${msg}`));
}

/**
 * Print an info message (cyan i).
 */
export function printInfo(msg: string): void {
  console.log(chalk.cyan(`ℹ ${msg}`));
}

/**
 * Print data as formatted JSON to stdout.
 */
export function printJson(data: unknown): void {
  console.log(JSON.stringify(data, null, 2));
}

/**
 * Print a warning message (yellow warning) to stderr.
 */
export function printWarning(msg: string): void {
  console.error(chalk.yellow(`⚠  ${msg}`));
}
