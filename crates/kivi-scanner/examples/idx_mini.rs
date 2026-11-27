//! Minimal repro: does one binary with one definition folder complete?
use kivi_scanner::pair_verify::verify_by_content;
use kivi_scanner::rom_identity::compare_identity;
use kivi_scanner::xdf_parse::parse_xdf_xml;
fn main() {
    let dir = "/root/ecu_master_database/01_MOTORCYCLES/HONDA/SPORT_CBR250RR";
    let bin = format!("{dir}/CBR250RR-38770-K64-ND1_BIN.bin");
    let xdf = format!("{dir}/CBR250RR_38770-K64-ND1_PUBLIC_v1.xdf");
    let t = std::time::Instant::now();
    let rom = std::fs::read(&bin).unwrap();
    println!("read bin {:?}", t.elapsed());
    let t = std::time::Instant::now();
    let bytes = std::fs::read(&xdf).unwrap();
    println!("read xdf {:?}", t.elapsed());
    let t = std::time::Instant::now();
    let parsed = parse_xdf_xml(&String::from_utf8_lossy(&bytes)).unwrap();
    println!("parse xdf {:?} ({} tables)", t.elapsed(), parsed.tables.len());
    let t = std::time::Instant::now();
    let _ = compare_identity(&rom, "x.xdf", "x");
    println!("identity {:?}", t.elapsed());
    let tables: Vec<_> = parsed.tables.iter().filter_map(|t|{
        let r=t.y_axis.as_ref().map(|a|a.index_count).unwrap_or(1);
        let c=t.x_axis.as_ref().map(|a|a.index_count).unwrap_or(1);
        if r<2&&c<2 {return None}
        Some((t.z_axis.address,r,c,t.z_axis.element_size_bits,t.z_axis.type_flags&0x02!=0,t.z_axis.major_stride_bits))
    }).collect();
    let t = std::time::Instant::now();
    let v = verify_by_content(&rom, &tables);
    println!("content {:?} -> {:?}", t.elapsed(), v);
}
