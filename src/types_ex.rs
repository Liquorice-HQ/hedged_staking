//
use crate::consts::*;
use crate::types::*;
use std::fmt;


//impl fmt::Display for Asset {
//    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
//        match self {
//            Asset::Primary(PrimaryAsset(amount)) => write!(f, "{} {}", amount, TRANSFERRING_COIN),
//            Asset::Secondary(SecondaryAsset(amount)) => write!(f, "{} {}", amount, OPERATING_COIN),
//        }
//    }
//}


impl fmt::Display for PrimaryAsset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(precision) = f.precision() {
            write!(f, "{:.*} {}", precision, self.0, TRANSFERRING_COIN)
        }
        else {
            write!(f, "{:.2} {}", self.0, TRANSFERRING_COIN)
        }
    }
}


impl fmt::Display for SecondaryAsset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(precision) = f.precision() {
            write!(f, "{:.*} {}", precision, self.0, OPERATING_COIN)
        }
        else {
            write!(f, "{:.2} {}", self.0, OPERATING_COIN)
        }
    }
}
