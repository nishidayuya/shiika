//use inkwell::values::*;
use crate::hir::*;
use crate::corelib::create_method;

pub fn create_methods() -> Vec<SkMethod> {
    vec![

    create_method("Shiika::Internal::Ptr", "+(n_bytes: Int) -> Shiika::Internal::Ptr", |code_gen, function| {
        let ptr = function.get_params()[0];
        let n_bytes = function.get_params()[1];
        let newptr = unsafe {
            code_gen.builder.build_gep(ptr.into_pointer_value(), &[n_bytes.into_int_value()], "newptr")
        };
        code_gen.builder.build_return(Some(&newptr));
        Ok(())
    }),

    ]
}


