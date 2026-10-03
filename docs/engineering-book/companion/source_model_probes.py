#!/usr/bin/env python3
"""Small executable source-model probes for Pentimento f819e59.

These are arithmetic and graph models of the inspected source, not compiled
Pentimento integration tests. They expose counterexamples to candidate claims;
they do not measure application performance or prove complete correctness.
Run with Python 3.10+; no third-party dependencies.
"""
import json
import math
from collections import defaultdict


def encode_normal(n):
    phi = math.acos(max(-1.0, min(1.0, n[2])))
    theta = math.atan2(n[1], n[0])
    return (min(int(phi / math.pi * 16), 15) << 4) | min(
        int((theta + math.pi) / math.tau * 16), 15)


def decode_normal(code):
    phi = (code >> 4) / 16 * math.pi
    theta = (code & 15) / 16 * math.tau
    return [math.sin(phi) * math.cos(theta),
            math.sin(phi) * math.sin(theta), math.cos(phi)]


def sculpt_spacing(samples, spacing):
    last_dab = 0.0
    accumulated = 0.0
    emitted = []
    for x in samples:
        distance = abs(x - last_dab)
        accumulated += distance
        if accumulated >= spacing:
            direction = 1 if x > last_dab else -1 if x < last_dab else 0
            current = last_dab
            while accumulated >= spacing:
                current += direction * spacing
                accumulated -= spacing
                emitted.append(current)
            last_dab = current
    return emitted


def build_halfedges(faces):
    halfedges = []
    outgoing = {}
    edge_map = {}
    for face_id, face in enumerate(faces):
        start = len(halfedges)
        for k, v in enumerate(face):
            he_id = len(halfedges)
            he = dict(origin=v, next=start+(k+1)%3, prev=start+(k-1)%3,
                      twin=None, face=face_id)
            halfedges.append(he)
            outgoing.setdefault(v, he_id)
        for k, v in enumerate(face):
            he_id = start+k
            dest = face[(k+1)%3]
            twin = edge_map.get((dest, v))
            if twin is not None:
                halfedges[he_id]['twin'] = twin
                halfedges[twin]['twin'] = he_id
            edge_map[v, dest] = he_id
    return halfedges, outgoing


def ring_walk(halfedges, outgoing, v):
    current = outgoing[v]
    visited = set()
    neighbors, faces = set(), set()
    while current not in visited and len(visited) < 100:
        visited.add(current)
        he = halfedges[current]
        neighbors.add(halfedges[he['next']]['origin'])
        faces.add(he['face'])
        twin = halfedges[he['prev']]['twin']
        if twin is None:
            break
        current = twin
    return neighbors, faces


def current_closed_manifold_model(faces):
    halfedges, outgoing = build_halfedges(faces)
    for i, he in enumerate(halfedges):
        twin = he['twin']
        if twin is None:
            raise ValueError('This probe is restricted to closed fixtures')
        if halfedges[twin]['twin'] != i:
            return False
        if halfedges[he['next']]['origin'] != halfedges[twin]['origin']:
            return False
    for v in outgoing:
        neighbors, incident = ring_walk(halfedges, outgoing, v)
        if max(0, len(neighbors)-len(incident)) != 0:
            return False
        if 0 < len(neighbors) < 3:
            return False
    return True


def independent_link_component_count(faces, v):
    # Independent definition: each incident triangle contributes the edge
    # opposite v to its vertex-link graph. Interior manifold links are cycles.
    graph = defaultdict(set)
    for face in faces:
        if v in face:
            a, b = [x for x in face if x != v]
            graph[a].add(b)
            graph[b].add(a)
    unseen = set(graph)
    count = 0
    while unseen:
        count += 1
        stack = [unseen.pop()]
        while stack:
            for w in graph[stack.pop()]:
                if w in unseen:
                    unseen.remove(w)
                    stack.append(w)
    return count


def greedy_face_survivors(face_order):
    owners, survivors = {}, []
    for face_id, (a, b, c) in face_order:
        rejected = False
        for e in [(a, b), (b, c), (c, a)]:
            if e in owners:
                rejected = True
                break
            owners[e] = face_id
        if not rejected:
            survivors.append(face_id)
    return survivors


def run():
    result = {'kind': 'source-model counterexamples',
              'repository_commit': 'f819e593819004690c4465afbb9733cb78b731c2',
              'compiled_pentimento_tests_run': False,
              'application_performance_measured': False, 'probes': []}
    normal = decode_normal(encode_normal((1, 0, 0)))
    dot = normal[0]
    assert dot < -0.999
    result['probes'].append(dict(id='normal_round_trip', input=[1, 0, 0],
        decoded=normal, dot_product=dot, claim='Source azimuth round trip reverses +X'))

    positions = [0.1, 0.2]
    previous = 0.0
    deltas = []
    for p in positions:
        deltas.append(int((p-previous)*100))
        previous = p
    serialized_base = int(previous*1000)/1000
    decoded = []
    cursor = serialized_base
    for delta in deltas:
        cursor += delta/100
        decoded.append(cursor)
    assert all(abs(a-b-0.2) < 1e-8 for a,b in zip(decoded, positions))
    result['probes'].append(dict(id='packet_base', input=positions,
        deltas=deltas, serialized_base=serialized_base, decoded=decoded,
        claim='Documented forward decoder starts from final rather than initial position'))

    samples = [0.02, 0.04, 0.06]
    dabs = sculpt_spacing(samples, 0.1)
    assert len(dabs) == 1 and dabs[0] > samples[-1]
    result['probes'].append(dict(id='resampling', samples=samples, spacing=0.1,
        emitted=dabs, claim='Repeated short inputs produce a dab beyond the pointer'))

    tetra = [(0,2,1), (0,1,3), (0,3,2), (1,2,3)]
    other = [(0,5,4), (0,4,6), (0,6,5), (4,5,6)]
    assert current_closed_manifold_model(tetra)
    assert independent_link_component_count(tetra, 0) == 1
    bowtie = tetra + other
    accepted = current_closed_manifold_model(bowtie)
    components = independent_link_component_count(bowtie, 0)
    assert accepted and components == 2
    result['probes'].append(dict(id='disconnected_vertex_link',
        triangles=bowtie, source_model_accepts=accepted,
        independent_link_components=components,
        claim='Two closed vertex fans pass the source-model check'))

    faces = [('A', (0,1,2)), ('B', (0,1,3))]
    ab, ba = greedy_face_survivors(faces), greedy_face_survivors(faces[::-1])
    assert ab != ba
    result['probes'].append(dict(id='compaction_order', order_ab_survives=ab,
        order_ba_survives=ba,
        claim='Greedy duplicate-edge deletion depends on traversal order'))
    return result


if __name__ == '__main__':
    print(json.dumps(run(), indent=2))
