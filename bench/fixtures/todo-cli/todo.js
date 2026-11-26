// Minimal todo list stored in todos.json.
import { readFileSync, writeFileSync, existsSync } from "node:fs";

const FILE = process.env.TODO_FILE ?? "todos.json";

export function load() {
  return existsSync(FILE) ? JSON.parse(readFileSync(FILE, "utf8")) : [];
}

export function save(todos) {
  writeFileSync(FILE, JSON.stringify(todos, null, 2));
}

export function add(todos, text) {
  const id = todos.length ? Math.max(...todos.map((t) => t.id)) + 1 : 1;
  return [...todos, { id, text }];
}

export function remove(todos, id) {
  return todos.filter((t) => t.id !== id);
}

export function format(todos) {
  return todos.map((t) => `${t.id}. ${t.text}`).join("\n");
}

if (process.argv[1] && process.argv[1].endsWith("todo.js")) {
  const [cmd, ...rest] = process.argv.slice(2);
  let todos = load();
  if (cmd === "add") {
    todos = add(todos, rest.join(" "));
    save(todos);
  } else if (cmd === "remove") {
    todos = remove(todos, Number(rest[0]));
    save(todos);
  }
  console.log(format(todos));
}
