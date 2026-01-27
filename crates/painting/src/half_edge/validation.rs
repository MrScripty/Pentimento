//! Validation methods for HalfEdgeMesh.

use super::types::HalfEdgeError;
use super::HalfEdgeMesh;

impl HalfEdgeMesh {
    /// Validate the mesh topology
    pub fn validate(&self) -> Result<(), HalfEdgeError> {
        // Check twin symmetry
        for he in &self.half_edges {
            if let Some(twin_id) = he.twin {
                let twin = self
                    .half_edge(twin_id)
                    .ok_or(HalfEdgeError::InvalidTopology("Invalid twin reference".into()))?;
                if twin.twin != Some(he.id) {
                    return Err(HalfEdgeError::InvalidTopology(
                        "Twin symmetry violated".into(),
                    ));
                }
            }
        }

        // Check next/prev cycle for each face
        for face in &self.faces {
            let start = face.half_edge;
            let mut current = start;
            let mut count = 0;

            loop {
                let he = self
                    .half_edge(current)
                    .ok_or(HalfEdgeError::InvalidTopology("Invalid half-edge".into()))?;

                if he.face != Some(face.id) {
                    return Err(HalfEdgeError::InvalidTopology(
                        "Half-edge face mismatch".into(),
                    ));
                }

                current = he.next;
                count += 1;

                if count > 1000 {
                    return Err(HalfEdgeError::InvalidTopology(
                        "Infinite loop in face".into(),
                    ));
                }

                if current == start {
                    break;
                }
            }

            if count < 3 {
                return Err(HalfEdgeError::InvalidTopology(
                    "Face has fewer than 3 edges".into(),
                ));
            }
        }

        Ok(())
    }
}
