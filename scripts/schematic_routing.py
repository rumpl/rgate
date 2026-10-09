"""Orthogonal obstacle-avoiding routes for generated examples (geometry only)."""
import bisect
import heapq


def body(gate):
    kind = gate["kind"]
    sizes = {"dip": (76, 22), "switch": (34, 26), "clock": (28, 26),
             "ground": (12, 12), "register": (76, 22), "mux": (60, 30),
             "add": (60, 30), "not": (20, 16), "buffer": (20, 16),
             "tri_state": (20, 16), "led": (14, 14)}
    if isinstance(kind, dict):
        width, height = 100, max([44] + [abs(pin["offset"]["y"]) * 2 + 24 for pin in gate["pins"]])
    elif kind in ("and", "or", "xor"):
        width, height = 26, max(16, gate["input_count"] * 5)
    elif kind == "comment":
        return None
    else:
        width, height = sizes.get(kind, (90, max([44] + [abs(pin["offset"]["y"]) * 2 + 20 for pin in gate["pins"]])))
    x, y = gate["position"]["x"], gate["position"]["y"]
    if kind == "led" and gate["config"].get("led_display", "bit") != "bit":
        mode = gate["config"]["led_display"]
        if mode == "bar":
            width, height = 4 + 6 * gate["width"], 16
        else:
            digits = (gate["width"] + 3) // 4 if mode == "hex" else len(str((1 << gate["width"]) - 1)) if mode == "decimal" else (gate["width"] + 6) // 7
            width, height = 4 + 24 * digits, 36
        ox, oy = 0, 7 - height / 2
        for _ in range(gate["rotation"]):
            ox, oy = oy, -ox
        x, y = x + ox, y + oy
    if gate["rotation"] % 2 and kind not in ("dip", "switch"):
        width, height = height, width
    return x - width / 2, y - height / 2, x + width / 2, y + height / 2


def point(gate, pin):
    x, y = pin["offset"]["x"], pin["offset"]["y"]
    for _ in range(gate["rotation"]):
        x, y = y, -x
    return gate["position"]["x"] + x, gate["position"]["y"] + y


def escape(gate, pin):
    x, y = point(gate, pin)
    x0, y0, x1, y1 = body(gate)
    # Standard primitives can have pins on top/bottom as well as sides.
    choices = [(abs(x-x0), (x0-12, y)), (abs(x-x1), (x1+12, y)),
               (abs(y-y0), (x, y0-12)), (abs(y-y1), (x, y1+12))]
    return min(choices, key=lambda choice: choice[0])[1]


def crosses(a, b, rectangle):
    x0, y0, x1, y1 = rectangle
    if a[0] == b[0]:
        return x0 < a[0] < x1 and max(min(a[1], b[1]), y0) < min(max(a[1], b[1]), y1)
    return y0 < a[1] < y1 and max(min(a[0], b[0]), x0) < min(max(a[0], b[0]), x1)


class Router:
    def __init__(self, gates):
        self.obstacles = []
        xs, ys = set(), set()
        for gate in gates:
            rectangle = body(gate)
            if rectangle is None:
                continue
            x0, y0, x1, y1 = rectangle
            rectangle = x0-4, y0-4, x1+4, y1+4
            self.obstacles.append(rectangle)
            xs.update((x0-12, x1+12))
            ys.update((y0-12, y1+12))
            for pin in gate["pins"]:
                x, y = escape(gate, pin)
                xs.add(x)
                ys.add(y)
        self.xs = sorted(xs)
        self.ys = sorted(ys)
        self.blocked = {}
        self.usage = {}

    def route(self, source, source_pin, target, target_pin):
        start, end = escape(source, source_pin), escape(target, target_pin)
        start_node = bisect.bisect_left(self.xs, start[0]), bisect.bisect_left(self.ys, start[1])
        end_node = bisect.bisect_left(self.xs, end[0]), bisect.bisect_left(self.ys, end[1])
        initial = (*start_node, 0)
        queue = [(0, 0, initial)]
        costs, previous = {initial: 0}, {}
        final = None
        while queue:
            _, cost, node = heapq.heappop(queue)
            if cost != costs[node]:
                continue
            ix, iy, direction = node
            if (ix, iy) == end_node:
                final = node
                break
            a = self.xs[ix], self.ys[iy]
            for dx, dy, axis in [(1, 0, 1), (-1, 0, 1), (0, 1, 2), (0, -1, 2)]:
                nx, ny = ix+dx, iy+dy
                if not (0 <= nx < len(self.xs) and 0 <= ny < len(self.ys)):
                    continue
                b = self.xs[nx], self.ys[ny]
                edge = tuple(sorted((a, b)))
                if edge not in self.blocked:
                    self.blocked[edge] = any(crosses(a, b, obstacle) for obstacle in self.obstacles)
                if self.blocked[edge]:
                    continue
                distance = abs(a[0]-b[0]) + abs(a[1]-b[1])
                new_cost = cost + distance + (18 if direction and direction != axis else 0) + self.usage.get(edge, 0) * distance * 0.8
                next_node = nx, ny, axis
                if new_cost < costs.get(next_node, float("inf")):
                    costs[next_node] = new_cost
                    previous[next_node] = node
                    heuristic = abs(b[0]-end[0]) + abs(b[1]-end[1])
                    heapq.heappush(queue, (new_cost+heuristic, new_cost, next_node))
        if final is None:
            raise ValueError(f"No obstacle-free route: {source['name']}:{source_pin['name']} -> {target['name']}:{target_pin['name']}")
        nodes = []
        while final != initial:
            nodes.append((self.xs[final[0]], self.ys[final[1]]))
            final = previous[final]
        nodes.append(start)
        nodes.reverse()
        for a, b in zip(nodes, nodes[1:]):
            edge = tuple(sorted((a, b)))
            self.usage[edge] = self.usage.get(edge, 0) + 1
        points = [point(source, source_pin)] + nodes + [point(target, target_pin)]
        simplified = []
        for p in points:
            if simplified and p == simplified[-1]:
                continue
            while len(simplified) >= 2 and ((simplified[-2][0] == simplified[-1][0] == p[0]) or (simplified[-2][1] == simplified[-1][1] == p[1])):
                simplified.pop()
            simplified.append(p)
        return [{"x": x, "y": y} for x, y in simplified]
