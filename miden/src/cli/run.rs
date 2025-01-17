use super::data::{InputFile, OutputFile, ProgramFile};
use crypto::Digest;
use std::path::PathBuf;
use std::time::Instant;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(name = "Run", about = "Run a miden program")]
pub struct RunCmd {
    /// Path to .masm assembly file
    #[structopt(short = "a", long = "assembly", parse(from_os_str))]
    assembly_file: PathBuf,
    /// Path to input file
    #[structopt(short = "i", long = "input", parse(from_os_str))]
    input_file: Option<PathBuf>,
    /// Number of ouptuts
    #[structopt(short = "n", long = "num-outputs", default_value = "16")]
    num_outputs: usize,
    /// Path to output file
    #[structopt(short = "o", long = "output", parse(from_os_str))]
    output_file: Option<PathBuf>,
}

impl RunCmd {
    pub fn execute(&self) -> Result<(), String> {
        println!("============================================================");
        println!("Run program");
        println!("============================================================");

        // load program from file and compile
        let program = ProgramFile::read(&self.assembly_file)?;

        // load input data from file
        let input_data = InputFile::read(&self.input_file, &self.assembly_file)?;

        print!(
            "Executing program with hash {}... ",
            hex::encode(program.hash().as_bytes())
        );
        let now = Instant::now();

        // execute program and generate outputs
        let trace = processor::execute(&program, &input_data.get_program_inputs())
            .map_err(|err| format!("Failed to generate exection trace = {:?}", err))?;

        println!("done ({} ms)", now.elapsed().as_millis());

        if let Some(output_path) = &self.output_file {
            // write outputs to file if one was specified
            OutputFile::write(trace.program_outputs(), output_path)?;
        } else {
            // write the stack outputs to the screen.
            println!(
                "Output: {:?}",
                trace.program_outputs().stack_outputs(self.num_outputs)
            );
        }

        Ok(())
    }
}
