use i_float::bit_pack::{BitPack, BitPackFix, BitPackVec};
use i_float::fix_vec::FixVec;
use i_float::triangle::Triangle;
use i_shape::index_point::IndexPoint;

use crate::{fill::segment::Segment};
use crate::index::EMPTY_INDEX;

use super::{overlay_node::OverlayNode, overlay_link::OverlayLink};

struct End {
    seg_index: usize,
    bit_pack: BitPack,
}

/// A representation of geometric shapes organized for efficient boolean operations.
///
/// `OverlayGraph` is a core structure designed to facilitate the execution of boolean operations on shapes, such as union, intersection, and difference. It organizes and preprocesses geometric data, making it optimized for these operations. This struct is the result of compiling shape data into a form where boolean operations can be applied directly, efficiently managing the complex relationships between different geometric entities.
///
/// Use `OverlayGraph` to perform boolean operations on the geometric shapes you've added to an `Overlay`, after it has processed the shapes according to the specified fill and overlay rules.
pub struct OverlayGraph {
    pub(crate) nodes: Vec<OverlayNode>,
    pub(crate) links: Vec<OverlayLink>,
}

impl OverlayGraph {
    // for js version
    pub fn links(&self) -> &Vec<OverlayLink> {
        &self.links
    }

    pub(super) fn new(segments: Vec<Segment>) -> Self {
        let n = segments.len();

        if n == 0 {
            return Self { nodes: vec![], links: vec![] };
        }

        let mut end_bs: Vec<End> = Vec::with_capacity(n);
        for (seg_index, segment) in segments.iter().enumerate() {
            end_bs.push(End {
                seg_index,
                bit_pack: segment.seg.b.bit_pack(),
            });
        }

        end_bs.sort_unstable_by(|a, b| a.bit_pack.cmp(&b.bit_pack));

        let mut nodes: Vec<OverlayNode> = Vec::with_capacity(2 * n);
        let mut links: Vec<OverlayLink> = segments
            .iter()
            .map(|segment| OverlayLink::new(IndexPoint::ZERO, IndexPoint::ZERO, segment.fill))
            .collect();

        let mut ai = 0;
        let mut bi = 0;
        let mut a = segments[0].seg.a.bit_pack();
        let mut b = end_bs[0].bit_pack;

        while ai < n || bi < n {
            let mut cnt = 0;
            if a == b {
                cnt += segments.size(a, ai);
                cnt += end_bs.size(b, bi);
            } else if ai < n && a < b {
                cnt += segments.size(a, ai);
            } else {
                cnt += end_bs.size(b, bi);
            }

            let mut indices = Vec::with_capacity(cnt);

            if a == b {
                let ip = IndexPoint::new(nodes.len(), a.fix_vec());
                while ai < n {
                    let aa = segments[ai].seg.a.bit_pack();
                    if aa != a {
                        a = aa;
                        break;
                    }
                    links[ai].a = ip;
                    indices.push(ai);

                    ai += 1
                }

                while bi < n {
                    let e = &end_bs[bi];
                    if e.bit_pack != b {
                        b = e.bit_pack;
                        break;
                    }
                    links[e.seg_index].b = ip;
                    indices.push(e.seg_index);

                    bi += 1
                }
            } else if ai < n && a < b {
                let ip = IndexPoint::new(nodes.len(), a.fix_vec());
                while ai < n {
                    let aa = segments[ai].seg.a.bit_pack();
                    if aa != a {
                        a = aa;
                        break;
                    }
                    links[ai].a = ip;
                    indices.push(ai);

                    ai += 1
                }
            } else {
                let ip = IndexPoint::new(nodes.len(), b.fix_vec());
                while bi < n {
                    let e = &end_bs[bi];
                    if e.bit_pack != b {
                        b = e.bit_pack;
                        break;
                    }
                    links[e.seg_index].b = ip;
                    indices.push(e.seg_index);

                    bi += 1
                }
            }

            debug_assert!(indices.len() > 1);
            nodes.push(OverlayNode { indices });
        }

        Self { nodes, links }
    }

    pub(crate) fn find_nearest_link_to(
        &self,
        target: IndexPoint,
        center: IndexPoint,
        ignore: usize,
        in_clockwise: bool,
        visited: &[bool],
    ) -> usize {
        let node = &self.nodes[center.index];

        let (index, value) = if let Some(result) = node.indices.iter().enumerate().find(|&(_index, &val)| val != ignore && !visited[val]) {
            (result.0, *result.1)
        } else {
            return EMPTY_INDEX;
        };

        let mut i = index + 1;
        let mut min_index = value;

        let mut min_vec = self.links[min_index].other(center).point - center.point;
        let v0 = target.point - center.point; // base vector

        // compare minVec with the rest of the vectors

        while i < node.indices.len() {
            let j = node.indices[i];
            if !visited[j] && ignore != j {
                let vj = self.links[j].other(center).point - center.point;

                if v0.is_closer_in_rotation_to(vj, min_vec) == in_clockwise {
                    min_vec = vj;
                    min_index = j;
                }
            }
            i += 1
        }

        min_index
    }

    pub(crate) fn find_first_link(&self, node_index: usize, visited: &Vec<bool>) -> usize {
        let node = &self.nodes[node_index];
        let mut j = EMPTY_INDEX;
        for &i in node.indices.iter() {
            if !visited[i] {
                if j == EMPTY_INDEX {
                    j = i;
                } else {
                    let a = self.links[j].a.point;
                    let bj = self.links[j].b.point;
                    let bi = self.links[i].b.point;

                    if Triangle::is_clockwise(a, bi, bj) {
                        j = i;
                    }
                }
            }
        }

        j
    }

    pub(crate) fn is_clockwise(a: FixVec, b: FixVec, is_top_inside: bool) -> bool {
        let is_direct = a.bit_pack() < b.bit_pack();

        Self::xnor(is_direct, is_top_inside)
    }

    fn xnor(a: bool, b: bool) -> bool {
        (a && b) || !(a || b)
    }
}

trait CloseInRotation {
    fn is_closer_in_rotation_to(&self, a: FixVec, b: FixVec) -> bool;
}

impl CloseInRotation for FixVec {
    // v, a, b vectors are multi-directional
    fn is_closer_in_rotation_to(&self, a: FixVec, b: FixVec) -> bool {
        let cross_a = self.cross_product(a);
        let cross_b = self.cross_product(b);

        if cross_a == 0 || cross_b == 0 {
            // vectors are collinear
            return if cross_a == 0 {
                // a is opposite to self, so based on cross_b
                cross_b > 0
            } else {
                // b is opposite to self, so based on cross_a
                cross_a < 0
            };
        }

        let same_side = (cross_a > 0 && cross_b > 0) || (cross_a < 0 && cross_b < 0);

        if !same_side {
            return cross_a < 0;
        }

        let cross_ab = a.cross_product(b);

        cross_ab < 0
    }
}

trait Size {
    fn size(&self, point: BitPack, index: usize) -> usize;
}

impl Size for Vec<Segment> {
    fn size(&self, point: BitPack, index: usize) -> usize {
        let mut i = index;
        while i < self.len() && self[i].seg.a.bit_pack() == point {
            i += 1;
        }
        i - index
    }
}

impl Size for Vec<End> {
    fn size(&self, point: BitPack, index: usize) -> usize {
        let mut i = index;
        while i < self.len() && self[i].bit_pack == point {
            i += 1;
        }
        i - index
    }
}