mod test_utils;

use bigdecimal::BigDecimal;
#[allow(unused_imports)]
use flexi_logger::ReconfigurationHandle;
use hdbconnect::{Connection, HdbResult, HdbValue};
use log::{debug, info};
use num::FromPrimitive;
use serde_derive::Deserialize;

//cargo test --test test_027_small_decimals -- --nocapture
#[test]
fn test_027_small_decimals() -> HdbResult<()> {
    let mut log_handle = test_utils::init_logger();
    let mut connection = test_utils::get_authenticated_connection()?;

    test_small_decimals(&mut log_handle, &mut connection)?;

    info!("{} calls to DB were executed", connection.get_call_count()?);
    Ok(())
}

fn test_small_decimals(
    _log_handle: &mut ReconfigurationHandle,
    connection: &mut Connection,
) -> HdbResult<()> {
    connection.multiple_statements_ignore_err(vec!["drop table TEST_SMALL_DECIMALS"]);

    let stmts = vec![
        "create table TEST_SMALL_DECIMALS (f1 NVARCHAR(100) primary key, f2 SMALLDECIMAL, \
        f3 integer, f2_NN SMALLDECIMAL NOT NULL, f3_NN integer NOT NULL)",
        "insert into TEST_SMALL_DECIMALS (f1, f2, f2_NN, f3_NN) values('0.00000', 0.000, 0.000, 10)",
        "insert into TEST_SMALL_DECIMALS (f1, f2, f2_NN, f3_NN) values('0.00100', 0.001, 0.001, 10)",
        "insert into TEST_SMALL_DECIMALS (f1, f2, f2_NN, f3_NN) values('-0.00100', -0.001, -0.001, 10)",
        "insert into TEST_SMALL_DECIMALS (f1, f2, f2_NN, f3_NN) values('0.00300', 0.003, 0.003, 10)",
        "insert into TEST_SMALL_DECIMALS (f1, f2, f2_NN, f3_NN) values('0.00700', 0.007, 0.007, 10)",
        "insert into TEST_SMALL_DECIMALS (f1, f2, f2_NN, f3_NN) values('0.25500', 0.255, 0.255, 10)",
        "insert into TEST_SMALL_DECIMALS (f1, f2, f2_NN, f3_NN) values('65.53500', 65.535, 65.535, 10)",
        "insert into TEST_SMALL_DECIMALS (f1, f2, f2_NN, f3_NN) values('-65.53500', -65.535, -65.535, 10)",
    ];
    connection.multiple_statements(stmts)?;

    #[derive(Deserialize)]
    struct TestData {
        #[serde(rename = "F1")]
        f1: String,
        #[serde(rename = "F2")]
        f2: BigDecimal,
        #[serde(rename = "F2_NN")]
        f2_nn: BigDecimal,
        #[serde(rename = "F3_NN")]
        f3_nn: u32,
    };

    let insert_stmt_str =
        "insert into TEST_SMALL_DECIMALS (F1, F2, F2_NN, F3_NN) values(?, ?, ?, ?)";

    // prepare & execute

    let mut insert_stmt = connection.prepare(insert_stmt_str)?;
    insert_stmt.add_batch(&(
        "75.53500",
        BigDecimal::from_f32(75.53500).unwrap(),
        BigDecimal::from_f32(75.53500).unwrap(),
        10,
    ))?;
    insert_stmt.add_batch(&("87.65432", 87.654_32_f32, 87.654_32_f32, 10))?;
    insert_stmt.add_batch(&("0.00500", 0.005_f32, 0.005_f32, 10))?;
    insert_stmt.add_batch(&("-0.00600", -0.006_00_f64, -0.006_00_f64, 10))?;
    insert_stmt.add_batch(&("-7.65432", -7.654_32_f64, -7.654_32_f64, 10))?;
    insert_stmt.add_batch(&("99.00000", 99, 99, 10))?;
    insert_stmt.add_batch(&("-50.00000", -50_i16, -50_i16, 10))?;
    insert_stmt.add_batch(&("22.00000", 22_i64, 22_i64, 10))?;
    insert_stmt.execute_batch()?;

    insert_stmt.add_batch(&("-0.05600", "-0.05600", "-0.05600", "10"))?;
    insert_stmt.add_batch(&("-8.65432", "-8.65432", "-8.65432", "10"))?;
    insert_stmt.execute_batch()?;

    info!("Read and verify decimals");
    let resultset =
        connection.query("select f1, f2, f3_NN, f2_NN from TEST_SMALL_DECIMALS order by f2")?;
    let precision = resultset.metadata().precision(1)?;
    debug!("metadata: {:?}", resultset.metadata());
    let scale = 5; //resultset.metadata().scale(1)? as usize;
    for row in resultset {
        let row = row?;
        if let HdbValue::DECIMAL(ref bd) = &row[1] {
            debug!("precision = {}, scale = {}", precision, scale);
            assert_eq!(format!("{}", &row[0]), format!("{0:.1$}", bd, scale));
        } else {
            assert!(false, "Unexpected value type");
        }
    }

    info!("Read and verify decimals to struct");
    let resultset =
        connection.query("select f1, f2, f3_NN, f2_NN from TEST_SMALL_DECIMALS order by f2")?;
    let scale = 5; //resultset.metadata().scale(1)? as usize;
    let result: Vec<TestData> = resultset.try_into()?;
    for td in result {
        debug!("{:?}, {:?}", td.f1, td.f2);
        assert_eq!(td.f1, format!("{}", format!("{0:.1$}", td.f2, scale)));
        assert_eq!(td.f1, format!("{}", format!("{0:.1$}", td.f2_nn, scale)));
        assert_eq!(td.f3_nn, 10);
    }

    info!("Read and verify small decimal to single value");
    let resultset = connection.query("select AVG(F3) from TEST_SMALL_DECIMALS")?;
    let mydata: Option<BigDecimal> = resultset.try_into()?;
    assert_eq!(mydata, None);

    // info!("Read and verify small decimal to single value");
    let resultset = connection.query("select AVG(F3_NN) from TEST_SMALL_DECIMALS")?;
    let mydata: BigDecimal = resultset.try_into()?;
    assert_eq!(mydata, BigDecimal::from_f32(10.0).unwrap());

    let mydata: Option<i64> = connection
        .query("select AVG(F2) from TEST_SMALL_DECIMALS where f2 = '65.53500'")?
        .try_into()?;
    assert_eq!(mydata, Some(65));

    let mydata: i64 = connection
        .query("select AVG(F2_NN) from TEST_SMALL_DECIMALS where f2_NN = '65.53500'")?
        .try_into()?;
    assert_eq!(mydata, 65);

    // test failing conversion
    let mydata: HdbResult<i8> = connection
        .query("select SUM(ABS(F2)) from TEST_SMALL_DECIMALS")?
        .try_into();
    assert!(mydata.is_err());

    // test failing conversion
    let mydata: HdbResult<i8> = connection
        .query("select SUM(ABS(F2_NN)) from TEST_SMALL_DECIMALS")?
        .try_into();
    assert!(mydata.is_err());

    // test working conversion
    let mydata: i64 = connection
        .query("select SUM(ABS(F2)) from TEST_SMALL_DECIMALS")?
        .try_into()?;
    assert_eq!(mydata, 481);

    // test working conversion
    let mydata: i64 = connection
        .query("select SUM(ABS(F2_NN)) from TEST_SMALL_DECIMALS")?
        .try_into()?;
    assert_eq!(mydata, 481);

    Ok(())
}
