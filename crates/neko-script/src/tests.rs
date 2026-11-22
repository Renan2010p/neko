//! End-to-end tests: GDScript programs and the output Godot 4 prints for them.

use std::rc::Rc;

use crate::interp::{Interp, NullHost};

/// Run `main()` of a script and return what it printed.
fn run(src: &str) -> String {
    run_files(&[("res://main.gd", src)])
}

fn run_files(files: &[(&str, &str)]) -> String {
    let mut it = Interp::new();
    let sources: Vec<(String, String)> = files.iter().map(|(p, s)| (p.to_string(), s.to_string())).collect();
    it.source = Box::new(move |p| sources.iter().find(|(q, _)| q == p).map(|(_, s)| s.clone()));
    // class_name scripts first, like Godot's global class cache.
    for (p, s) in files.iter().skip(1) {
        it.compile(p, s).unwrap_or_else(|e| panic!("{e}"));
    }
    let class = it.compile(files[0].0, files[0].1).unwrap_or_else(|e| panic!("{e}"));
    let obj = it.instantiate(&class, None).unwrap_or_else(|e| panic!("{e}"));
    if let Err(e) = it.call_method(&obj, "main", &[]) {
        let host = it.host.take().unwrap();
        let out = output(host);
        panic!("{e}\n--- output so far:\n{out}");
    }
    output(it.host.take().unwrap())
}

fn output(host: Box<dyn crate::interp::Host>) -> String {
    // NullHost is the only host in tests.
    let raw = Box::into_raw(host) as *mut NullHost;
    // SAFETY: tests always install NullHost (Interp::new default).
    let h = unsafe { Box::from_raw(raw) };
    h.output.join("\n")
}

#[test]
fn arithmetic_and_printing() {
    let out = run(r#"
func main():
	print(7 / 2, " ", 7.0 / 2, " ", 7 % 3, " ", 2 ** 10, " ", -7 / 2)
	print(1.0, " ", 0.5, " ", 1e3, " ", 10 / 4.0)
	print(Vector2(1, 2) * 2, " ", Vector2i(3, 4), " ", Vector3(1, 2, 3).length())
	print("%s has %d HP (%.1f%%)" % ["Cat", 9, 87.5])
	print(str(true) + "," + str(null) + "," + str([1, "a", 2.5]))
	print({"a": 1, "b": [2]})
	var x: float = 3
	print(x / 2)
	var n: int = 7.9
	print(n)
"#);
    assert_eq!(out, "3 3.5 1 1024 -3\n1.0 0.5 1000.0 2.5\n(2.0, 4.0) (3, 4) 3.74165749549866\nCat has 9 HP (87.5%)\ntrue,<null>,[1, \"a\", 2.5]\n{ \"a\": 1, \"b\": [2] }\n1.5\n7");
}

#[test]
fn collections() {
    let out = run(r#"
func main():
	var a := [3, 1, 2]
	a.sort()
	a.append(10)
	print(a, " ", a.size(), " ", a.has(2), " ", a.find(10))
	print(a.map(func(x): return x * 2), a.filter(func(x): return x > 2), a.reduce(func(acc, x): return acc + x, 0))
	var d := {"hp": 3}
	d["mp"] = 5
	d.erase("hp")
	print(d, " ", d.keys(), " ", d.get("x", 42), " ", "mp" in d)
	var b = a
	b.push_back(99)
	print(a.back(), " ", a.duplicate() == a)
	var words := "a,b,,c".split(",")
	print(words, " ", ",".join(["x", "y"]), " ", "Hello".to_upper(), " ", "  hi ".strip_edges(), " ", "abc".substr(1, 1))
	var total := 0
	for i in range(5):
		if i == 3:
			continue
		total += i
	for k in {"x": 1, "y": 2}:
		total += 100
	var w := 0
	while w < 3:
		w += 1
	print(total, " ", w)
"#);
    assert_eq!(out, "[1, 2, 3, 10] 4 true 3\n[2, 4, 6, 20][3, 10]16\n{ \"mp\": 5 } [\"mp\"] 42 true\n99 true\n[\"a\", \"b\", \"\", \"c\"] x,y HELLO hi b\n207 3");
}

#[test]
fn classes_inheritance_and_signals() {
    let out = run_files(&[
        (
            "res://main.gd",
            r#"
extends "res://base.gd"

signal scored(points)

enum State { IDLE, RUN = 5, JUMP }
const SPEED := 2.5
var hp: int = 10:
	set(value):
		hp = clampi(value, 0, 20)
		print("hp -> ", hp)
	get:
		return hp
var log := []

class Bag:
	var items := []
	func add(x):
		items.append(x)
		return self

static func twice(x):
	return x * 2

func greet():
	return "child+" + super()

func main():
	print(greet(), " ", describe())
	hp = 50
	hp -= 30
	print(hp, " ", State.RUN, " ", State.JUMP, " ", SPEED, " ", twice(4))
	scored.connect(func(p): log.append(p))
	scored.connect(_on_scored)
	scored.emit(3)
	print(log)
	var bag = Bag.new()
	bag.add(1).add(2)
	print(bag.items, " ", bag is Bag, " ", self is Base)
	var counter := 0
	var inc := func(): counter += 1; return counter
	print(inc.call(), " ", inc.call(), " ", counter)

func _on_scored(p):
	log.append(p * 10)
"#,
        ),
        (
            "res://base.gd",
            r#"
class_name Base
var name_tag := "base"
func greet():
	return "base"
func describe():
	return "I am " + name_tag
"#,
        ),
    ]);
    assert_eq!(out, "child+base I am base\nhp -> 20\nhp -> 0\n0 5 6 2.5 8\n[3, 30]\n[1, 2] true true\n1 1 0");
}

#[test]
fn match_patterns() {
    let out = run(r#"
func kind(v):
	match v:
		0, 1:
			return "small"
		"cat":
			return "string"
		[var a, var b]:
			return "pair %s %s" % [a, b]
		[1, ..]:
			return "starts with 1"
		{"type": "enemy", "hp": var hp}:
			return "enemy %d" % hp
		var n when n is int and n > 100:
			return "big"
		_:
			return "other"

func main():
	for v in [1, "cat", [4, 5], [1, 2, 3], {"type": "enemy", "hp": 7}, 500, 50, 1.0]:
		print(kind(v))
"#);
    assert_eq!(out, "small\nstring\npair 4 5\nstarts with 1\nenemy 7\nbig\nother\nother");
}

#[test]
fn transforms_and_aabb() {
    let out = run(r#"
func main():
	var t := Transform3D.IDENTITY
	print(t * Vector3(1, 2, 3))
	t.origin = Vector3(10, 20, 30)
	print(t * Vector3(1, 0, 0))
	print(Basis.IDENTITY.scaled(Vector3(2, 3, 4)) * Vector3.ONE)
	var box := AABB(Vector3(1, 2, 3), Vector3(4, 5, 6))
	print(box.end)
	print(t.basis.x)
"#);
    assert_eq!(out, "(1.0, 2.0, 3.0)\n(11.0, 20.0, 30.0)\n(2.0, 3.0, 4.0)\n(5.0, 7.0, 9.0)\n(1.0, 0.0, 0.0)");
}

#[test]
fn runtime_errors_report_line() {
    let mut it = Interp::new();
    let c = it.compile("res://bad.gd", "func main():\n\tvar a = [1]\n\tprint(a[5])\n").unwrap();
    let o = it.instantiate(&c, None).unwrap();
    let e = it.call_method(&o, "main", &[]).unwrap_err();
    assert!(e.message.contains("Out of bounds"), "{e}");
    assert_eq!((e.file.as_str(), e.line, e.func.as_str()), ("res://bad.gd", 3, "main"));
    let _ = Rc::new(0);
}

#[test]
fn packed_byte_array_binary_access() {
    let out = run(r#"
func main():
	var b := PackedByteArray()
	b.resize(8)
	b.encode_s16(0, -2)
	b.encode_u16(2, 65535)
	b.encode_s32(4, -123456)
	print(b.decode_s16(0), " ", b.decode_u16(2), " ", b.decode_s32(4), " ", b.size())
"#);
    assert_eq!(out, "-2 65535 -123456 8");
}

#[test]
fn await_suspends_and_resumes_on_signal() {
    use crate::value::Value;
    let mut it = Interp::new();
    let src = r#"
extends Node
signal go
var hits := 0

func wait_one() -> int:
	await go
	return 7

func main():
	await go
	hits += 1
	var x = await wait_one()
	print("after ", x, " ", hits)
"#;
    let class = it.compile("res://main.gd", src).unwrap();
    let obj = it.instantiate(&class, None).unwrap();
    it.call_method(&obj, "main", &[]).unwrap();
    // While waiting, nothing after the await has run.
    let out = output(it.host.take().unwrap());
    assert!(out.is_empty(), "{out}");
    it.host = Some(Box::new(NullHost::default()));
    it.emit(&obj, "go", &[Value::Nil]).unwrap();
    it.run_tasks();
    // wait_one starts on the second await; its own signal resumes it.
    it.emit(&obj, "go", &[Value::Nil]).unwrap();
    it.run_tasks();
    let out = output(it.host.take().unwrap());
    assert_eq!(out.lines().last(), Some("after 7 1"));
}

#[test]
fn await_in_conditions_and_loops() {
    use crate::value::Value;
    let mut it = Interp::new();
    let src = r#"
extends Node
signal go
var turns := 0

func main():
	if await go:
		turns += 1
	while await go:
		turns += 1
		if turns >= 3:
			break
	print("turns ", turns)
"#;
    let class = it.compile("res://main.gd", src).unwrap();
    let obj = it.instantiate(&class, None).unwrap();
    it.call_method(&obj, "main", &[]).unwrap();
    for _ in 0..4 {
        it.emit(&obj, "go", &[Value::Bool(true)]).unwrap();
        it.run_tasks();
    }
    let out = output(it.host.take().unwrap());
    assert_eq!(out.lines().last(), Some("turns 3"));
}

#[test]
fn base_method_calls_the_derived_override() {
    let out = run_files(&[
        ("res://main.gd", "func main():\n\tvar d := Derived.new()\n\tprint(d.call_build())\n"),
        (
            "res://base.gd",
            "class_name Base\nextends RefCounted\nfunc build() -> String:\n\treturn \"base\"\nfunc call_build() -> String:\n\treturn build()\n",
        ),
        (
            "res://derived.gd",
            "class_name Derived\nextends Base\nfunc build() -> String:\n\treturn \"derived\"\n",
        ),
    ]);
    assert_eq!(out, "derived");
}

#[test]
fn metadata_is_stored_per_object() {
    let out = run(r#"
func main():
	var n := Node2D.new()
	n.set_meta("kind", "save")
	n.set_meta("at", Vector2(3, 4))
	print(n.get_meta("kind", ""), " ", n.has_meta("kind"), " ", n.get_meta("missing", "dflt"))
	n.remove_meta("kind")
	print(n.has_meta("kind"))
"#);
    assert_eq!(out, "save true dflt\nfalse");
}
